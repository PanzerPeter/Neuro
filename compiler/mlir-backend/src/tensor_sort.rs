// `.sort()`, `.argsort()` and `.topk()` as `linalg`, for GPU bodies.
//
// A rank sort in two steps. First every element counts the elements of its run that come
// before it: each one that precedes it under the comparator, and each equal one at an
// earlier position. That count is exactly where a stable sort puts the element, so this
// computes the LLVM backend's stable insertion sort in parallel, the same permutation with
// ties and NaNs included. Its index space is the source's axes (`parallel`, one GPU thread
// per element) followed by the compared element's position along the sorted axis
// (`reduction`, a loop inside the thread). Then every output position takes the one
// element whose count is its position: the output's axes (`parallel`) followed by the run
// (`reduction`). `.topk` is a descending sort with `k` output positions per run, so it
// gathers those and never builds the rest of the order.
//
// ponytail: O(extent^2) comparisons per run, one thread per element. A radix or bitonic
// sort is the upgrade, once runs too long for this are measured.
//
// Each destination is seeded before its step, because a reduction reads it at every
// point: the counts with 0, and the outputs with 0, which every position then overwrites
// exactly once, since the counts of a run are a permutation of its positions.

use crate::{
    errors::MlirError,
    lower::map_type,
    tensor_arithmetic::{
        Generic, OperandAxes, build_expression, empty_tensor, fill_block, indexing_maps,
        iterator_types, tensor_parts,
    },
    tensor_reduce::{append, apply, yield_value},
};

use melior::{
    Context,
    dialect::arith::{self, CmpfPredicate, CmpiPredicate},
    ir::{
        Attribute, Block, BlockLike, Identifier, Location, Operation, Type, Value,
        attribute::{FloatAttribute, IntegerAttribute},
        operation::OperationBuilder,
        r#type::{IntegerType, RankedTensorType},
    },
};
use neuro_hir::{HirExpr, HirExprKind, HirSortKind, HirTarget, HirType};

/// The width of the integer attribute `linalg.index` names its dimension with.
const DIMENSION_BITS: u32 = 64;

/// What one output of a selection takes from the element its position finds.
#[derive(Clone, Copy)]
enum Picked {
    /// The element itself: `.sort()`, and `.topk`'s values.
    Element,
    /// The element's position in its run: `.argsort()`, and `.topk`'s indices.
    Position,
}

/// Lower `sort`, a `TensorSort`, into one tensor per result: the sorted elements, the
/// order, or `.topk`'s values and then its indices.
pub(crate) fn build_sort<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    block: &'a Block<'c>,
    sort: &HirExpr,
    scope: &[(String, Value<'c, 'a>)],
    target: HirTarget,
) -> Result<Option<Vec<Value<'c, 'a>>>, MlirError> {
    let HirExprKind::TensorSort {
        receiver,
        kind,
        axis,
        descending,
    } = &sort.kind
    else {
        return Ok(None);
    };
    let Some((element, shape)) = tensor_parts(&receiver.ty) else {
        return Ok(None);
    };
    let Some(extents) = shape.iter().copied().collect::<Option<Vec<usize>>>() else {
        return Ok(None);
    };
    if !matches!(element, HirType::F32 | HirType::F64) || *axis >= extents.len() {
        return Ok(None);
    }
    let outputs: Vec<(Picked, &HirType)> = match (kind, &sort.ty) {
        (HirSortKind::Values, ty) => vec![(Picked::Element, ty)],
        (HirSortKind::Indices, ty) => vec![(Picked::Position, ty)],
        (HirSortKind::TopK(_), HirType::Tuple(parts)) => match parts.as_slice() {
            [values, indices] => vec![(Picked::Element, values), (Picked::Position, indices)],
            _ => return Ok(None),
        },
        (HirSortKind::TopK(_), _) => return Ok(None),
    };
    let Some(source) = build_expression(context, location, block, receiver, scope, target)? else {
        return Ok(None);
    };

    let element_type = map_type(context, element)?;
    let counts = count(
        context,
        location,
        block,
        source,
        &extents,
        *axis,
        element_type,
        *descending,
    )?;
    let mut results = Vec::with_capacity(outputs.len());
    for (picked, ty) in outputs {
        let Some(result) = gather(
            context,
            location,
            block,
            (source, element_type),
            counts,
            ty,
            *axis,
            picked,
        )?
        else {
            return Ok(None);
        };
        results.push(result);
    }
    Ok(Some(results))
}

/// Each source element's position in its sorted run.
#[expect(clippy::too_many_arguments)]
fn count<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    block: &'a Block<'c>,
    source: Value<'c, 'a>,
    extents: &[usize],
    axis: usize,
    element: Type<'c>,
    descending: bool,
) -> Result<Value<'c, 'a>, MlirError> {
    let rank = extents.len();
    let index = Type::index(context);
    let shape: Vec<u64> = extents.iter().map(|&extent| extent as u64).collect();
    let counts_type = RankedTensorType::new(&shape, index, None).into();
    let own: OperandAxes = Some((0..rank).map(Some).collect());
    let seeded = seed(
        context,
        location,
        block,
        counts_type,
        IntegerAttribute::new(index, 0).into(),
        index,
        rank,
    )?;
    apply(
        context,
        location,
        block,
        Generic {
            inputs: &[source, source],
            destination: seeded,
            indexing_maps: indexing_maps(context, rank + 1, &[&own, &along_run(rank, axis), &own])?,
            iterators: iterator_types(context, rank + 1, 1)?,
        },
        counts_type,
        count_block(context, location, element, axis, rank, descending)?,
    )
}

/// One output of the selection, of type `result`: at each position, what `picked` takes
/// from the element counted there.
#[expect(clippy::too_many_arguments)]
fn gather<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    block: &'a Block<'c>,
    (source, source_element): (Value<'c, 'a>, Type<'c>),
    counts: Value<'c, 'a>,
    result: &HirType,
    axis: usize,
    picked: Picked,
) -> Result<Option<Value<'c, 'a>>, MlirError> {
    let Some((element, shape)) = tensor_parts(result) else {
        return Ok(None);
    };
    let rank = shape.len();
    let tensor_type = map_type(context, result)?;
    let element_type = map_type(context, element)?;
    let zero: Attribute = match picked {
        Picked::Element => FloatAttribute::new(context, element_type, 0.0).into(),
        Picked::Position => IntegerAttribute::new(element_type, 0).into(),
    };
    let seeded = seed(
        context,
        location,
        block,
        tensor_type,
        zero,
        element_type,
        rank,
    )?;
    let run = along_run(rank, axis);
    let own: OperandAxes = Some((0..rank).map(Some).collect());
    Ok(Some(apply(
        context,
        location,
        block,
        Generic {
            inputs: &[source, counts],
            destination: seeded,
            indexing_maps: indexing_maps(context, rank + 1, &[&run, &run, &own])?,
            iterators: iterator_types(context, rank + 1, 1)?,
        },
        tensor_type,
        gather_block(
            context,
            location,
            source_element,
            element_type,
            axis,
            rank,
            picked,
        )?,
    )?))
}

/// A fresh tensor of `tensor_type` holding `value` everywhere.
fn seed<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    block: &'a Block<'c>,
    tensor_type: Type<'c>,
    value: Attribute<'c>,
    element: Type<'c>,
    rank: usize,
) -> Result<Value<'c, 'a>, MlirError> {
    let empty = append(block, empty_tensor(location, tensor_type, &[])?)?;
    let value = append(block, arith::constant(context, value, location))?;
    let own: OperandAxes = Some((0..rank).map(Some).collect());
    apply(
        context,
        location,
        block,
        Generic {
            inputs: &[value],
            destination: empty,
            indexing_maps: indexing_maps(context, rank, &[&None, &own])?,
            iterators: iterator_types(context, rank, 0)?,
        },
        tensor_type,
        fill_block(location, element)?,
    )
}

/// A tensor read with its sorted axis walked by the index space's last dimension, the run
/// position, and every other axis by its own.
fn along_run(rank: usize, axis: usize) -> OperandAxes {
    Some(
        (0..rank)
            .map(|a| Some(if a == axis { rank } else { a }))
            .collect(),
    )
}

/// One counting step over `(element, compared, count)`. A compared element at an earlier
/// position adds one unless the element precedes it, which keeps equal elements in their
/// order; one at a later position (or the element itself) adds one only if it precedes the
/// element.
fn count_block<'c>(
    context: &'c Context,
    location: Location<'c>,
    element: Type<'c>,
    axis: usize,
    run: usize,
    descending: bool,
) -> Result<Block<'c>, MlirError> {
    let index = Type::index(context);
    let block = Block::new(&[(element, location), (element, location), (index, location)]);
    let mine: Value = block.argument(0)?.into();
    let theirs: Value = block.argument(1)?.into();
    let counted: Value = block.argument(2)?.into();
    let position = append(&block, linalg_index(context, location, axis)?)?;
    let compared = append(&block, linalg_index(context, location, run)?)?;
    let earlier = append(
        &block,
        arith::cmpi(context, CmpiPredicate::Ult, compared, position, location),
    )?;
    let mine_first = precedes(context, location, &block, mine, theirs, descending)?;
    let theirs_first = precedes(context, location, &block, theirs, mine, descending)?;
    let one = append(
        &block,
        arith::constant(context, IntegerAttribute::new(index, 1).into(), location),
    )?;
    let zero = append(
        &block,
        arith::constant(context, IntegerAttribute::new(index, 0).into(), location),
    )?;
    let if_earlier = append(&block, arith::select(mine_first, zero, one, location))?;
    let if_later = append(&block, arith::select(theirs_first, one, zero, location))?;
    let step = append(
        &block,
        arith::select(earlier, if_earlier, if_later, location),
    )?;
    let total = append(&block, arith::addi(counted, step, location))?;
    yield_value(&block, location, total)?;
    Ok(block)
}

/// One gathering step over `(element, count, output)`: the output position whose number
/// is the element's count takes what `picked` names, and every other keeps its value.
fn gather_block<'c>(
    context: &'c Context,
    location: Location<'c>,
    source: Type<'c>,
    output: Type<'c>,
    axis: usize,
    run: usize,
    picked: Picked,
) -> Result<Block<'c>, MlirError> {
    let index = Type::index(context);
    let block = Block::new(&[(source, location), (index, location), (output, location)]);
    let counted: Value = block.argument(1)?.into();
    let kept: Value = block.argument(2)?.into();
    let position = append(&block, linalg_index(context, location, axis)?)?;
    let hit = append(
        &block,
        arith::cmpi(context, CmpiPredicate::Eq, counted, position, location),
    )?;
    let value = match picked {
        Picked::Element => block.argument(0)?.into(),
        Picked::Position => {
            let at = append(&block, linalg_index(context, location, run)?)?;
            append(&block, arith::index_cast(at, output, location))?
        }
    };
    let written = append(&block, arith::select(hit, value, kept, location))?;
    yield_value(&block, location, written)?;
    Ok(block)
}

/// The current position along index-space dimension `dimension`.
fn linalg_index<'c>(
    context: &'c Context,
    location: Location<'c>,
    dimension: usize,
) -> Result<Operation<'c>, MlirError> {
    let width = IntegerType::new(context, DIMENSION_BITS).into();
    Ok(OperationBuilder::new("linalg.index", location)
        .add_attributes(&[(
            Identifier::new(context, "dim"),
            IntegerAttribute::new(width, dimension as i64).into(),
        )])
        .add_results(&[Type::index(context)])
        .build()?)
}

/// Whether `a` strictly precedes `b` under the LLVM backend's sorting comparator: in
/// order by `<` (by `>` when `descending`), with a NaN sorting last in either direction,
/// so it precedes nothing and everything else precedes it.
pub(crate) fn precedes<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    block: &'a Block<'c>,
    a: Value<'c, 'a>,
    b: Value<'c, 'a>,
    descending: bool,
) -> Result<Value<'c, 'a>, MlirError> {
    let predicate = if descending {
        CmpfPredicate::Ogt
    } else {
        CmpfPredicate::Olt
    };
    let ordered = append(block, arith::cmpf(context, predicate, a, b, location))?;
    let b_nan = append(
        block,
        arith::cmpf(context, CmpfPredicate::Uno, b, b, location),
    )?;
    let a_real = append(
        block,
        arith::cmpf(context, CmpfPredicate::Ord, a, a, location),
    )?;
    let before = append(block, arith::ori(ordered, b_nan, location))?;
    append(block, arith::andi(a_real, before, location))
}

#[cfg(test)]
mod tests {
    use crate::{GpuTarget, lower_for_gpu, lower_for_link};

    use neuro_hir::{HirItem, HirProgram, HirTarget};

    /// A program with a transfer, so lowering outlines its operations. The harness binds
    /// no named arguments, so `k` and the axis are positional.
    fn program(body: &str) -> HirProgram {
        let source = format!(
            "enum Device {{\n    CPU,\n    GPU(i32)\n}}\n\nfunc main() -> i32 {{\n    val m: Tensor<f64, [37, 19]> = Tensor::ones()\n    val g = m.clone().to(Device::GPU(0))\n{body}\n    return 0\n}}\n"
        );
        let ast = syntax_parsing::parse(&source).expect("the program parses");
        hir_lowering::lower_program(&ast).expect("the program lowers to HIR")
    }

    fn nvidia() -> GpuTarget {
        GpuTarget::Nvidia {
            chip: "sm_80".to_string(),
        }
    }

    #[test]
    fn every_selection_counts_once_and_gathers_per_output() {
        for (body, launches) in [
            ("    val s = g.sort()", 4),
            ("    val s = g.argsort(0)", 4),
            ("    val (v, i) = g.topk(3)", 6),
        ] {
            let bodies = lower_for_gpu(&program(body), &nvidia()).expect("a selection lowers");
            assert_eq!(bodies.functions.len(), 1, "`{body}`");
            let ir = &bodies.llvm_ir;
            assert_eq!(
                ir.matches("call void @mgpuLaunchKernel").count(),
                launches,
                "`{body}`: a seed and a count, then a seed and a gather per output:\n{ir}"
            );
        }
    }

    #[test]
    fn topk_writes_its_two_results_through_two_out_params() {
        let bodies =
            lower_for_gpu(&program("    val (v, i) = g.topk(3)"), &nvidia()).expect("lowers");
        let (_, symbol) = &bodies.functions[0];
        let ir = &bodies.llvm_ir;
        let start = ir
            .find(&format!("define void @{symbol}("))
            .unwrap_or_else(|| panic!("`{symbol}` is defined:\n{ir}"));
        let signature = &ir[start..start + ir[start..].find('{').unwrap_or(0)];
        // Three rank-2 descriptors of seven words each: the receiver, the values, the
        // indices.
        assert_eq!(signature.matches("ptr").count(), 6, "{signature}");
        assert_eq!(signature.matches("i64").count(), 15, "{signature}");
    }

    #[test]
    fn the_cpu_path_leaves_selections_to_the_llvm_backend() {
        let mut program = program("    val s = g.sort()\n    val (v, i) = g.topk(3)");
        for item in &mut program.items {
            if let HirItem::Function(f) = item {
                f.target = HirTarget::Host;
            }
        }
        let bodies = lower_for_link(&program).expect("the CPU path lowers");
        assert!(bodies.functions.is_empty(), "{:?}", bodies.functions);
    }
}
