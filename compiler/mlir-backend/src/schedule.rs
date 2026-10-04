// How a contraction runs fast without changing a bit: a transform-dialect schedule.
//
// A contraction (`@`, and an `einsum` with a contracted letter) is built as one
// `linalg.generic` whose loops lower to the naive nest: each result element in turn, its
// contracted letters innermost, so the accumulator round-trips through memory and the
// right operand of a matrix product is walked down a column. The schedule tiles the result
// axes into a register block, gives every contracted letter a loop of its own outside that
// block, vectorizes the block and hoists its accumulator into the contracted loops'
// iteration arguments, so it lives in registers from the first product to the last.
//
// Every result element still adds its products one at a time in the contracted letters'
// order, starting from the zero its fill wrote, with a multiply and an add each rounded
// on its own: the block is a vector of independent accumulators, never a split sum. That
// is the order the 2026-10-03 ruling fixes for every backend, so the bits are the naive
// nest's on the host and on a GPU. No tensor-core MMA and no vendor BLAS: both split the
// contracted extent or fuse the multiply into the add, and both change the last bits.
//
// The builder tags the contraction with its tile sizes (`tag`), chosen from its static
// extents; `apply` reads the tags back out of the built module and runs one script over
// it before the bufferizing pipeline. A host block is 4 rows by 64 bytes of columns,
// which fills the sixteen 16-byte registers the baseline x86-64 target has; an axis the
// block does not divide is peeled, so the main loop's blocks are static and vectorize and
// the remainder is one smaller static block. A GPU block is up to 4 x 4 per thread, sized
// to divide its axes: an `scf.forall` cannot be peeled, and a partial block would not
// vectorize, leaving a loop nest inside every thread.

use std::fmt::Write;

use melior::{
    Context,
    ir::{
        Attribute, BlockLike, Module, Operation,
        attribute::DenseI64ArrayAttribute,
        operation::{OperationLike, OperationMutLike, WalkOrder, WalkResult},
    },
};

use neuro_hir::HirType;

use crate::{errors::MlirError, guards::Side};

/// Tile size per loop of a scheduled contraction, result axes first: its register block,
/// then a 0 for each contracted letter, which the first tiling leaves whole.
const TILES: &str = "neuro.tiles";

/// The positions, among the tiled loops, of the ones whose step does not divide their axis.
const PEEL: &str = "neuro.peel";

/// Set on every scheduled contraction so its tiles are found again however many copies
/// tiling and peeling made of it.
const VECTORIZE: &str = "neuro.vectorize";

/// A host block's rows.
const HOST_ROWS: u64 = 4;

/// The bytes of columns a host block spans: four 16-byte registers a row.
const HOST_ROW_BYTES: u64 = 64;

/// The largest block edge a GPU thread computes, on each of the two innermost axes.
const DEVICE_EDGE: u64 = 4;

/// Tag `contraction`, a contraction over `extents` (its loop extents in iterator order,
/// result axes first, `parallel` of them) of `element`s, with its [`plan`].
///
/// Its body must be pure arithmetic: a checked integer body reports through a status word,
/// which no vectorized block writes.
pub(crate) fn tag<'c>(
    context: &'c Context,
    contraction: &mut Operation<'c>,
    side: Side,
    extents: (&[u64], usize),
    element: &HirType,
) {
    let Some(Schedule { tiles, peel }) = plan(side, extents, element) else {
        return;
    };
    contraction.set_attribute(TILES, DenseI64ArrayAttribute::new(context, &tiles).into());
    contraction.set_attribute(PEEL, DenseI64ArrayAttribute::new(context, &peel).into());
    contraction.set_attribute(VECTORIZE, Attribute::unit(context));
}

/// The schedule for a contraction over `extents` with `parallel` result axes, or `None`
/// where the naive nest is as good or the block would not vectorize.
fn plan(side: Side, (extents, parallel): (&[u64], usize), element: &HirType) -> Option<Schedule> {
    let (result, contracted) = extents.split_at_checked(parallel)?;
    if result.is_empty() || contracted.is_empty() {
        return None;
    }
    let block = match side {
        Side::Host => [HOST_ROWS, HOST_ROW_BYTES / element_bytes(element)],
        Side::Device => [DEVICE_EDGE, DEVICE_EDGE],
    };
    // The block covers the innermost result axes; a rank-1 result takes its columns, the
    // edge that vectorizes.
    let covered = result.len().min(block.len());
    let block = &block[block.len() - covered..];
    let leading = result.len() - covered;
    let tiles: Vec<u64> = result
        .iter()
        .enumerate()
        .map(|(axis, &extent)| {
            let Some(edge) = axis.checked_sub(leading) else {
                return 1;
            };
            match side {
                Side::Host => block[edge].min(extent),
                Side::Device => (1..=block[edge].min(extent))
                    .rev()
                    .find(|edge| extent % edge == 0)
                    .unwrap_or(1),
            }
        })
        .collect();
    if tiles.iter().all(|&tile| tile <= 1) {
        return None;
    }
    // One block covering the whole result is a one-iteration `scf.forall`, which folds
    // into its body and would leave the block in the host launcher, reading device memory.
    if side == Side::Device
        && tiles
            .iter()
            .zip(result)
            .all(|(tile, extent)| tile == extent)
    {
        return None;
    }
    let peel = tiles
        .iter()
        .zip(result)
        .enumerate()
        .filter(|(_, (tile, extent))| **extent % **tile != 0)
        .map(|(position, _)| position as i64)
        .collect();
    let tiles = tiles
        .iter()
        .map(|&tile| tile as i64)
        .chain(contracted.iter().map(|_| 0))
        .collect();

    Some(Schedule { tiles, peel })
}

/// The bytes one element takes in a register.
fn element_bytes(element: &HirType) -> u64 {
    match element {
        HirType::I8 | HirType::U8 => 1,
        HirType::I16 | HirType::U16 => 2,
        HirType::I32 | HirType::U32 | HirType::F32 => 4,
        _ => 8,
    }
}

/// One schedule: the contractions tagged with these tile sizes and peeled loops.
#[derive(Debug, PartialEq)]
struct Schedule {
    tiles: Vec<i64>,
    peel: Vec<i64>,
}

/// Run every tagged contraction's schedule over `module`, before it is bufferized.
///
/// # Errors
///
/// [`MlirError::PassPipelineFailed`] if a schedule does not apply, which is a bug in the
/// tags: every one is static and sized for its own contraction.
pub(crate) fn apply(context: &Context, module: &Module<'_>, side: Side) -> Result<(), MlirError> {
    let schedules = schedules(module);
    if schedules.is_empty() {
        return Ok(());
    }
    let script = Module::parse(context, &script(&schedules, side))
        .ok_or(MlirError::ModuleVerificationFailed)?;
    let entry = script
        .body()
        .first_operation()
        .ok_or(MlirError::ModuleVerificationFailed)?;

    // SAFETY: the payload and the script are live modules of the same context, and the entry
    // is the script's own first operation, so all three outlive the call. The options are
    // created here, used only by this call and destroyed after it.
    let applied = unsafe {
        let options = mlir_sys::mlirTransformOptionsCreate();
        let result = mlir_sys::mlirTransformApplyNamedSequence(
            module.as_operation().to_raw(),
            entry.to_raw(),
            script.as_operation().to_raw(),
            options,
        );
        mlir_sys::mlirTransformOptionsDestroy(options);
        result
    };
    if applied.value == 0 {
        return Err(MlirError::PassPipelineFailed);
    }

    Ok(())
}

/// The distinct schedules `module`'s tags ask for, in the order first met.
fn schedules(module: &Module<'_>) -> Vec<Schedule> {
    let mut found = Vec::new();
    module
        .as_operation()
        .walk(WalkOrder::PreOrder, |operation| {
            let read = |name| {
                let array =
                    DenseI64ArrayAttribute::try_from(operation.attribute(name).ok()?).ok()?;
                (0..array.len()).map(|at| array.element(at).ok()).collect()
            };
            if let (Some(tiles), Some(peel)) = (read(TILES), read(PEEL)) {
                let schedule = Schedule { tiles, peel };
                if !found.contains(&schedule) {
                    found.push(schedule);
                }
            }
            WalkResult::Advance
        });
    found
}

/// The transform module: tile each schedule's contractions, peel what does not divide, then
/// vectorize every block, lower its transfers' permutations, and hoist each accumulator into
/// its contracted loops.
fn script(schedules: &[Schedule], side: Side) -> String {
    const ANY: &str = "!transform.any_op";
    let mut text = String::from(
        "module attributes {transform.with_named_sequence} {\n\
         transform.named_sequence @__transform_main(%root: !transform.any_op {transform.readonly}) {\n",
    );
    for (index, schedule) in schedules.iter().enumerate() {
        let list = |values: &[i64]| {
            values
                .iter()
                .map(i64::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        };
        let results = |count: usize| vec![ANY; count].join(", ");
        let contracted = schedule.tiles.iter().filter(|&&tile| tile == 0).count();
        let tiled_loops = schedule.tiles.len() - contracted;
        let _ = writeln!(
            text,
            "%op{index} = transform.structured.match ops{{[\"linalg.generic\"]}} attributes{{{TILES} = array<i64: {tiles}>, {PEEL} = array<i64{peel}>}} in %root : ({ANY}) -> {ANY}",
            tiles = list(&schedule.tiles),
            peel = if schedule.peel.is_empty() {
                String::new()
            } else {
                format!(": {}", list(&schedule.peel))
            },
        );
        match side {
            Side::Host => {
                let _ = writeln!(
                    text,
                    "%block{index}, %loop{index}:{tiled_loops} = transform.structured.tile_using_for %op{index} tile_sizes [{tiles}] : ({ANY}) -> ({ANY}, {loops})",
                    tiles = list(&schedule.tiles),
                    loops = results(tiled_loops),
                );
            }
            Side::Device => {
                let _ = writeln!(
                    text,
                    "%block{index}, %loop{index} = transform.structured.tile_using_forall %op{index} tile_sizes [{tiles}] : ({ANY}) -> ({ANY}, {ANY})",
                    tiles = list(&schedule.tiles),
                );
            }
        }
        let per_letter: Vec<i64> = schedule
            .tiles
            .iter()
            .map(|&tile| i64::from(tile == 0))
            .collect();
        let _ = writeln!(
            text,
            "%step{index}, %letter{index}:{contracted} = transform.structured.tile_using_for %block{index} tile_sizes [{steps}] : ({ANY}) -> ({ANY}, {loops})",
            steps = list(&per_letter),
            loops = results(contracted),
        );
        // Innermost first: peeling an outer loop copies the loops inside it, and a copy of
        // one already peeled is peeled too.
        for position in schedule.peel.iter().rev() {
            let _ = writeln!(
                text,
                "%for{index}_{position} = transform.cast %loop{index}#{position} : {ANY} to !transform.op<\"scf.for\">\n\
                 %main{index}_{position}, %rest{index}_{position} = transform.loop.peel %for{index}_{position} : (!transform.op<\"scf.for\">) -> ({ANY}, {ANY})",
            );
        }
    }
    let _ = write!(
        text,
        "%functions = transform.structured.match ops{{[\"func.func\"]}} in %root : ({ANY}) -> {ANY}\n\
         transform.apply_patterns to %functions {{\n\
           transform.apply_patterns.canonicalization\n\
         }} : {ANY}\n\
         %blocks = transform.structured.match ops{{[\"linalg.generic\"]}} attributes{{{VECTORIZE}}} in %root : ({ANY}) -> {ANY}\n\
         transform.structured.vectorize %blocks : {ANY}\n\
         transform.apply_patterns to %functions {{\n\
           transform.apply_patterns.vector.transfer_permutation_patterns\n\
           transform.apply_patterns.vector.cast_away_vector_leading_one_dim\n\
           transform.apply_patterns.vector.drop_unit_dims_with_shape_cast\n\
           transform.apply_patterns.canonicalization\n\
         }} : {ANY}\n\
         %loops = transform.structured.match ops{{[\"scf.for\"]}} in %root : ({ANY}) -> {ANY}\n\
         transform.loop.hoist_loop_invariant_subsets %loops : {ANY}\n\
         transform.yield\n\
         }}\n\
         }}\n"
    );
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::context::new_context;

    fn schedule(tiles: &[i64], peel: &[i64]) -> Option<Schedule> {
        Some(Schedule {
            tiles: tiles.to_vec(),
            peel: peel.to_vec(),
        })
    }

    #[test]
    fn a_host_block_is_four_rows_of_sixty_four_bytes_peeled_where_it_does_not_divide() {
        let f32_product = plan(Side::Host, (&[37, 45, 19], 2), &HirType::F32);
        assert_eq!(f32_product, schedule(&[4, 16, 0], &[0, 1]));
        let f64_product = plan(Side::Host, (&[64, 64, 9], 2), &HirType::F64);
        assert_eq!(f64_product, schedule(&[4, 8, 0], &[]));
    }

    #[test]
    fn a_rank_one_result_blocks_its_columns_and_leading_axes_stay_single() {
        let vector = plan(Side::Host, (&[100, 7], 1), &HirType::F32);
        assert_eq!(vector, schedule(&[16, 0], &[0]));
        let batched = plan(Side::Host, (&[3, 8, 32, 5, 6], 3), &HirType::I32);
        assert_eq!(batched, schedule(&[1, 4, 16, 0, 0], &[]));
    }

    #[test]
    fn a_device_block_divides_its_axes_and_never_covers_the_whole_result() {
        let product = plan(Side::Device, (&[36, 6, 20], 2), &HirType::F32);
        assert_eq!(product, schedule(&[4, 3, 0], &[]));
        assert_eq!(plan(Side::Device, (&[2, 4, 3], 2), &HirType::F32), None);
        assert_eq!(plan(Side::Device, (&[37, 41, 3], 2), &HirType::F32), None);
    }

    #[test]
    fn contractions_without_a_reduction_are_not_blocked() {
        assert_eq!(plan(Side::Host, (&[64, 64], 2), &HirType::F32), None);
    }

    #[test]
    fn every_script_parses() {
        let context = new_context();
        let host = [
            plan(Side::Host, (&[37, 45, 19], 2), &HirType::F32),
            plan(Side::Host, (&[3, 8, 32, 5, 6], 3), &HirType::I32),
            plan(Side::Host, (&[100, 7], 1), &HirType::F64),
        ];
        let device = [
            plan(Side::Device, (&[36, 6, 20], 2), &HirType::F32),
            plan(Side::Device, (&[2, 8, 12, 4], 3), &HirType::I64),
        ];
        for (side, plans) in [
            (Side::Host, Vec::from(host)),
            (Side::Device, Vec::from(device)),
        ] {
            let count = plans.len();
            let schedules: Vec<Schedule> = plans.into_iter().flatten().collect();
            assert_eq!(schedules.len(), count);
            let text = script(&schedules, side);
            assert!(Module::parse(&context, &text).is_some(), "{text}");
        }
    }
}
