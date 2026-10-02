// `.sum()`, `.mean()`, `.max()` and `.min()` as `linalg`, for GPU bodies.
//
// A reduction is a `linalg.generic` whose index space is the result's axes followed by the
// reduced ones: the result axes are `parallel` and become GPU threads, the reduced ones are
// `reduction` and become a sequential loop inside each thread. So every result element is
// folded by one thread in the source's own order, which is the order the LLVM backend folds
// in, and the two give the same bits. A whole-tensor reduction writes a one-element tensor
// (its one parallel axis has extent 1), since a GPU body returns buffers only.
//
// A run longer than `REDUCE_LANES` folds in the language's lane order first: one more
// all-parallel `linalg.generic` over the result's axes and a lane axis, one GPU thread per
// lane, each running an `scf.for` over its run positions `lane, lane + REDUCE_LANES, ...`
// into a partials tensor. Adjacent lanes read adjacent elements, so a warp's loads coalesce.
// The fold above then reduces the lane axis, exactly as the LLVM backend's lane order does,
// so even a whole-tensor sum keeps the host's bits while running on thousands of threads.
// The lane generic reads the source with `tensor.extract` at a position it computes, so it
// has no input for the elementwise fusion to fold into the next generic.
//
// The destination is seeded before the fold, because a reduction reads it at every point:
// with `-0.0` for a sum (the one float that adds as nothing, signed zeros included), and
// with the run's first element for `.max()` / `.min()`, which the fold then meets again
// harmlessly. That is the LLVM backend's own start, so a NaN or an all-equal run picks the
// same element on both.

use crate::{
    errors::MlirError,
    lower::map_type,
    tensor_arithmetic::{
        Generic, OperandAxes, build_expression, empty_tensor, fill_block, generic_op,
        indexing_maps, iterator_types, tensor_parts,
    },
    tensor_layout::linalg_index,
    tensor_sort::precedes,
};

use melior::{
    Context,
    dialect::arith::{self, CmpiPredicate},
    ir::{
        Block, BlockLike, Location, Region, RegionLike, Type, Value,
        attribute::{FloatAttribute, IntegerAttribute},
        operation::OperationBuilder,
        r#type::RankedTensorType,
    },
};
use neuro_hir::{HirExpr, HirExprKind, HirReduceOp, HirTarget, HirType, REDUCE_LANES};

/// How many region arguments a fold body takes: the source element, then the accumulator.
const FOLD_BODY_ARGUMENTS: usize = 2;

/// The seed of a sum: adding it to any float gives that float back, `-0.0` included.
const SUM_SEED: f64 = -0.0;

/// Lower `reduce`, a `TensorReduce`, into a tensor of type `result`: the reduction's own
/// type for an axis, or a one-element tensor for a whole-tensor reduction.
pub(crate) fn build_reduce<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    block: &'a Block<'c>,
    reduce: &HirExpr,
    result: &HirType,
    scope: &[(String, Value<'c, 'a>)],
    target: HirTarget,
) -> Result<Option<Value<'c, 'a>>, MlirError> {
    let HirExprKind::TensorReduce { receiver, op, axis } = &reduce.kind else {
        return Ok(None);
    };
    let (Some((element, source)), Some((_, extents))) =
        (tensor_parts(&receiver.ty), tensor_parts(result))
    else {
        return Ok(None);
    };
    let Some(layout) = ReduceLayout::new(source, extents, *axis) else {
        return Ok(None);
    };
    if !matches!(element, HirType::F32 | HirType::F64) {
        return Ok(None);
    }
    let Some(source) = build_expression(context, location, block, receiver, scope, target)? else {
        return Ok(None);
    };

    let tensor_type = map_type(context, result)?;
    let element_type = map_type(context, element)?;
    let length = layout.length;
    let (source, layout) = match layout.lanes {
        Some(ref run) if length > REDUCE_LANES => {
            let partials = build_lanes(context, location, block, source, run, *op, element_type)?;
            let Some(lanes) = ReduceLayout::new(&run.partials(), extents, Some(run.result_rank))
            else {
                return Ok(None);
            };
            (partials, lanes)
        }
        _ => (source, layout),
    };
    let parallel = layout.result_rank;
    let result_axes: OperandAxes = Some((0..parallel).map(Some).collect());
    let empty = block
        .append_operation(empty_tensor(location, tensor_type, &[])?)
        .result(0)?
        .into();

    let (seed_input, seed_axes) = match op {
        HirReduceOp::Sum | HirReduceOp::Mean => {
            let seed = FloatAttribute::new(context, element_type, SUM_SEED).into();
            let seed = block
                .append_operation(arith::constant(context, seed, location))
                .result(0)?
                .into();
            (seed, None)
        }
        HirReduceOp::Max | HirReduceOp::Min => (source, Some(layout.first.clone())),
    };
    let seeded = apply(
        context,
        location,
        block,
        Generic {
            inputs: &[seed_input],
            destination: empty,
            indexing_maps: indexing_maps(context, parallel, &[&seed_axes, &result_axes])?,
            iterators: iterator_types(context, parallel, 0)?,
        },
        tensor_type,
        fill_block(location, element_type)?,
    )?;

    let folded = apply(
        context,
        location,
        block,
        Generic {
            inputs: &[source],
            destination: seeded,
            indexing_maps: indexing_maps(
                context,
                layout.space,
                &[&Some(layout.walk.clone()), &result_axes],
            )?,
            iterators: iterator_types(context, layout.space, layout.space - parallel)?,
        },
        tensor_type,
        fold_block(context, location, element_type, *op)?,
    )?;
    if *op != HirReduceOp::Mean {
        return Ok(Some(folded));
    }

    let length = FloatAttribute::new(context, element_type, length as f64).into();
    let length = block
        .append_operation(arith::constant(context, length, location))
        .result(0)?
        .into();
    Ok(Some(apply(
        context,
        location,
        block,
        Generic {
            inputs: &[length],
            destination: folded,
            indexing_maps: indexing_maps(context, parallel, &[&None, &result_axes])?,
            iterators: iterator_types(context, parallel, 0)?,
        },
        tensor_type,
        mean_block(location, element_type)?,
    )?))
}

/// Where one reduction's index space puts each axis.
struct ReduceLayout {
    /// The index space's rank: the result's axes, then the reduced ones.
    space: usize,
    result_rank: usize,
    /// The index-space dimension each source axis walks during the fold.
    walk: Vec<Option<usize>>,
    /// Each source axis read over the result's index space when taking a run's first
    /// element: the result axis it matches, or element 0 of a reduced axis.
    first: Vec<Option<usize>>,
    /// How many elements one run folds, which a mean divides by.
    length: usize,
    /// Where a run's elements sit, for folding a long run in lanes.
    lanes: Option<Run>,
}

/// The source shape and reduced axis, which place each run position in the source.
struct Run {
    extents: Vec<usize>,
    /// The reduced axis; `None` for a whole-tensor reduction, whose run is the source in
    /// row-major order.
    axis: Option<usize>,
    result_rank: usize,
}

impl Run {
    /// The partials tensor's extents: the result's, then one per lane.
    fn partials(&self) -> Vec<Option<usize>> {
        let mut extents: Vec<Option<usize>> = match self.axis {
            None => vec![Some(1)],
            Some(axis) => (0..self.extents.len())
                .filter(|&i| i != axis)
                .map(|i| Some(self.extents[i]))
                .collect(),
        };
        extents.push(Some(REDUCE_LANES));
        extents
    }
}

impl ReduceLayout {
    /// `None` for anything but a static source reduced to the result shape it implies.
    fn new(
        source: &[Option<usize>],
        result: &[Option<usize>],
        axis: Option<usize>,
    ) -> Option<Self> {
        let extents = source.iter().copied().collect::<Option<Vec<usize>>>()?;
        let rank = extents.len();
        if rank == 0 {
            return None;
        }
        let Some(axis) = axis else {
            // One parallel axis of extent 1, then every source axis reduced.
            return (result == [Some(1)]).then(|| Self {
                space: rank + 1,
                result_rank: 1,
                walk: (1..=rank).map(Some).collect(),
                first: vec![None; rank],
                length: extents.iter().product(),
                lanes: Some(Run {
                    extents: extents.clone(),
                    axis: None,
                    result_rank: 1,
                }),
            });
        };
        let kept: Vec<Option<usize>> = (0..rank)
            .filter(|&i| i != axis)
            .map(|i| Some(extents[i]))
            .collect();
        if axis >= rank || kept != result || kept.is_empty() {
            return None;
        }
        let result_axis = |i: usize| (i != axis).then(|| if i < axis { i } else { i - 1 });
        Some(Self {
            space: rank,
            result_rank: rank - 1,
            walk: (0..rank)
                .map(|i| Some(result_axis(i).unwrap_or(rank - 1)))
                .collect(),
            first: (0..rank).map(result_axis).collect(),
            length: extents[axis],
            lanes: Some(Run {
                extents: extents.clone(),
                axis: Some(axis),
                result_rank: rank - 1,
            }),
        })
    }
}

/// Fold a long run's lanes into a partials tensor of the result's axes and a lane axis.
/// Lane `l` starts at run position `l` (inside the run, which is longer than the lanes) and
/// folds `l + k * REDUCE_LANES` for `k` from 1 while that is inside the run, the order the
/// LLVM backend's lanes take.
fn build_lanes<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    block: &'a Block<'c>,
    source: Value<'c, 'a>,
    run: &Run,
    op: HirReduceOp,
    element_type: Type<'c>,
) -> Result<Value<'c, 'a>, MlirError> {
    let index = Type::index(context);
    let length = run.extents.iter().product::<usize>();
    let length = match run.axis {
        Some(axis) => run.extents[axis],
        None => length,
    };
    // Every extent is static: `ReduceLayout::new` admits only a static source.
    let dimensions: Vec<u64> = run
        .partials()
        .iter()
        .map(|extent| extent.map_or(0, |extent| extent as u64))
        .collect();
    let partials_type = RankedTensorType::new(&dimensions, element_type, None).into();
    let rank = dimensions.len();
    let empty = append(block, empty_tensor(location, partials_type, &[])?)?;

    let body = Block::new(&[(element_type, location)]);
    // The result position, one `linalg.index` per surviving source axis.
    let kept: Vec<Value> = (0..run.result_rank)
        .map(|dimension| append(&body, linalg_index(context, location, dimension)?))
        .collect::<Result<_, _>>()?;
    let lane = append(&body, linalg_index(context, location, run.result_rank)?)?;
    let seed = read_run(
        context,
        location,
        &body,
        source,
        run,
        &kept,
        lane,
        element_type,
    )?;
    let first = index_constant(context, location, &body, 1)?;
    let rows = index_constant(context, location, &body, length.div_ceil(REDUCE_LANES))?;

    let step = Block::new(&[(index, location), (element_type, location)]);
    let row: Value = step.argument(0)?.into();
    let carried: Value = step.argument(1)?.into();
    let width = index_constant(context, location, &step, REDUCE_LANES)?;
    let scaled = append(&step, arith::muli(row, width, location))?;
    let position = append(&step, arith::addi(scaled, lane, location))?;
    let end = index_constant(context, location, &step, length)?;
    let inside = append(
        &step,
        arith::cmpi(context, CmpiPredicate::Ult, position, end, location),
    )?;
    // Past the run's end the lane reads its own first element and keeps its accumulator.
    let at = append(&step, arith::select(inside, position, lane, location))?;
    let value = read_run(
        context,
        location,
        &step,
        source,
        run,
        &kept,
        at,
        element_type,
    )?;
    let folded = fold_step(context, location, &step, op, value, carried)?;
    let kept_value = append(&step, arith::select(inside, folded, carried, location))?;
    step.append_operation(
        OperationBuilder::new("scf.yield", location)
            .add_operands(&[kept_value])
            .build()?,
    );
    let region = Region::new();
    region.append_block(step);
    let lane_value = append(
        &body,
        OperationBuilder::new("scf.for", location)
            .add_operands(&[first, rows, first, seed])
            .add_results(&[element_type])
            .add_regions([region])
            .build()?,
    )?;
    yield_value(&body, location, lane_value)?;

    let own = Some((0..rank).map(Some).collect());
    apply(
        context,
        location,
        block,
        Generic {
            inputs: &[],
            destination: empty,
            indexing_maps: indexing_maps(context, rank, &[&own])?,
            iterators: iterator_types(context, rank, 0)?,
        },
        partials_type,
        body,
    )
}

/// The source element at run position `position` of the run whose result position is
/// `kept`: along the reduced axis for an axis reduction, and row-major through every axis
/// for a whole-tensor one.
#[expect(clippy::too_many_arguments)]
fn read_run<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    at: &'a Block<'c>,
    source: Value<'c, '_>,
    run: &Run,
    kept: &[Value<'c, '_>],
    position: Value<'c, '_>,
    element_type: Type<'c>,
) -> Result<Value<'c, 'a>, MlirError> {
    let constant = |value| index_constant(context, location, at, value);
    let mut operands: Vec<Value> = vec![source];
    match run.axis {
        // `kept` holds one index per surviving axis, in order, so axis `i` past the reduced
        // one is `kept[i - 1]`.
        Some(axis) => operands.extend((0..run.extents.len()).map(|i| match i.cmp(&axis) {
            std::cmp::Ordering::Less => kept[i],
            std::cmp::Ordering::Equal => position,
            std::cmp::Ordering::Greater => kept[i - 1],
        })),
        None => {
            let mut stride: usize = run.extents.iter().product();
            for (i, extent) in run.extents.iter().enumerate() {
                stride /= extent;
                let mut coordinate = position;
                if stride > 1 {
                    let divisor = constant(stride)?;
                    coordinate = append(at, arith::divui(coordinate, divisor, location))?;
                }
                if i > 0 {
                    let modulus = constant(*extent)?;
                    coordinate = append(at, arith::remui(coordinate, modulus, location))?;
                }
                operands.push(coordinate);
            }
        }
    }
    append(
        at,
        OperationBuilder::new("tensor.extract", location)
            .add_operands(&operands)
            .add_results(&[element_type])
            .build()?,
    )
}

fn index_constant<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    at: &'a Block<'c>,
    value: usize,
) -> Result<Value<'c, 'a>, MlirError> {
    let index = Type::index(context);
    append(
        at,
        arith::constant(
            context,
            IntegerAttribute::new(index, value as i64).into(),
            location,
        ),
    )
}

/// Append one `linalg.generic` with `body` as its single block, yielding its result.
pub(crate) fn apply<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    block: &'a Block<'c>,
    generic: Generic<'c, '_>,
    tensor_type: Type<'c>,
    body: Block<'c>,
) -> Result<Value<'c, 'a>, MlirError> {
    let region = Region::new();
    region.append_block(body);
    Ok(block
        .append_operation(generic_op(context, location, generic, tensor_type, region)?)
        .result(0)?
        .into())
}

/// One fold step over `(element, accumulator)`. A sum adds the element to the
/// accumulator. `.max()` / `.min()` keep the element when it is a number that sorts
/// before the accumulator, or when the accumulator is NaN: the LLVM backend's sorting
/// comparator, so a NaN never wins and a NaN seed gives way to the first number.
fn fold_block<'c>(
    context: &'c Context,
    location: Location<'c>,
    element: Type<'c>,
    op: HirReduceOp,
) -> Result<Block<'c>, MlirError> {
    let block = Block::new(&[(element, location); FOLD_BODY_ARGUMENTS]);
    let value: Value = block.argument(0)?.into();
    let carried: Value = block.argument(1)?.into();
    let folded = fold_step(context, location, &block, op, value, carried)?;
    yield_value(&block, location, folded)?;
    Ok(block)
}

/// `value` folded into `carried`, appended to `block`.
fn fold_step<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    block: &'a Block<'c>,
    op: HirReduceOp,
    value: Value<'c, '_>,
    carried: Value<'c, '_>,
) -> Result<Value<'c, 'a>, MlirError> {
    let descending = match op {
        HirReduceOp::Sum | HirReduceOp::Mean => {
            return append(block, arith::addf(carried, value, location));
        }
        HirReduceOp::Max => true,
        HirReduceOp::Min => false,
    };
    let wins = precedes(context, location, block, value, carried, descending)?;
    append(block, arith::select(wins, value, carried, location))
}

/// A mean's last step over `(length, total)`: the total divided by the run length.
fn mean_block<'c>(location: Location<'c>, element: Type<'c>) -> Result<Block<'c>, MlirError> {
    let block = Block::new(&[(element, location); FOLD_BODY_ARGUMENTS]);
    let mean = append(
        &block,
        arith::divf(
            block.argument(1)?.into(),
            block.argument(0)?.into(),
            location,
        ),
    )?;
    yield_value(&block, location, mean)?;
    Ok(block)
}

pub(crate) fn append<'c, 'a>(
    block: &'a Block<'c>,
    operation: melior::ir::Operation<'c>,
) -> Result<Value<'c, 'a>, MlirError> {
    Ok(block.append_operation(operation).result(0)?.into())
}

pub(crate) fn yield_value<'c>(
    block: &Block<'c>,
    location: Location<'c>,
    value: Value<'c, '_>,
) -> Result<(), MlirError> {
    block.append_operation(
        OperationBuilder::new("linalg.yield", location)
            .add_operands(&[value])
            .build()?,
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{GpuTarget, lower_for_gpu, lower_for_link};

    use neuro_hir::{HirItem, HirProgram, HirTarget};

    /// A program with a transfer, so lowering outlines its operations. The harness binds
    /// no named arguments, so an axis is positional.
    fn program(body: &str) -> HirProgram {
        let source = format!(
            "enum Device {{\n    CPU,\n    GPU(i32)\n}}\n\nfunc main() -> i32 {{\n    val m: Tensor<f32, [37, 19]> = Tensor::ones()\n    val g = m.clone().to(Device::GPU(0))\n{body}\n    return 0\n}}\n"
        );
        let ast = syntax_parsing::parse(&source).expect("the program parses");
        hir_lowering::lower_program(&ast).expect("the program lowers to HIR")
    }

    fn nvidia() -> GpuTarget {
        GpuTarget::Nvidia {
            chip: "sm_80".to_string(),
        }
    }

    fn outlined(program: &HirProgram) -> usize {
        program
            .items
            .iter()
            .filter(|item| {
                matches!(item, HirItem::Function(f) if f.target == HirTarget::FollowsOperands)
            })
            .count()
    }

    #[test]
    fn every_reduction_becomes_a_kernel_per_step() {
        for (body, launches) in [
            ("    val r = g.sum(1)", 2),
            ("    val r = g.mean(0)", 3),
            ("    val r = g.max()", 2),
            ("    val r = g.min(1)", 2),
        ] {
            let program = program(body);
            let bodies = lower_for_gpu(&program, &nvidia()).expect("a reduction lowers");
            assert_eq!(bodies.functions.len(), 1, "`{body}`");
            let ir = &bodies.llvm_ir;
            assert_eq!(
                ir.matches("call void @mgpuLaunchKernel").count(),
                launches,
                "`{body}`: a seed, a fold, and a mean's division:\n{ir}"
            );
        }
    }

    #[test]
    fn a_run_longer_than_the_lanes_folds_its_lanes_first() {
        // One more kernel than a short run: the lane partials, then the seed and the fold
        // over the lane axis (and a mean's division).
        for (shape, body, launches) in [
            ("[5000]", "    val r = g.sum()", 3),
            ("[3, 9000]", "    val r = g.mean(1)", 4),
            ("[9000, 3]", "    val r = g.max(0)", 3),
            ("[4096, 2]", "    val r = g.sum(0)", 2),
        ] {
            let source = format!(
                "enum Device {{\n    CPU,\n    GPU(i32)\n}}\n\nfunc main() -> i32 {{\n    val m: Tensor<f32, {shape}> = Tensor::ones()\n    val g = m.clone().to(Device::GPU(0))\n{body}\n    return 0\n}}\n"
            );
            let ast = syntax_parsing::parse(&source).expect("the program parses");
            let program = hir_lowering::lower_program(&ast).expect("the program lowers to HIR");
            let ir = lower_for_gpu(&program, &nvidia())
                .expect("a reduction lowers")
                .llvm_ir;
            assert_eq!(
                ir.matches("call void @mgpuLaunchKernel").count(),
                launches,
                "`{body}` over {shape}:\n{ir}"
            );
        }
    }

    #[test]
    fn a_max_keeps_the_host_comparator() {
        let ir = lower_for_gpu(&program("    val r = g.max()"), &nvidia())
            .expect("a reduction lowers")
            .llvm_ir;
        assert!(
            ir.contains("setp.gt.f32") || ir.contains("setp.gtu.f32"),
            "{ir}"
        );
        assert!(
            ir.contains("setp.nan.f32"),
            "a NaN accumulator gives way:\n{ir}"
        );
    }

    #[test]
    fn a_body_the_gpu_path_cannot_take_keeps_its_host_body_without_error() {
        // A rank-1 axis reduction has a rank-0 result, so it stays inline; an integer one
        // is never outlined. Neither reaches the GPU, and neither is an error.
        let program = program(
            "    val v: Tensor<f32, [4]> = Tensor::ones()\n    val s = v.sum(0)\n    val i: Tensor<i32, [4]> = Tensor::ones()\n    val t = &i + &i",
        );
        assert_eq!(outlined(&program), 0);
        let bodies = lower_for_gpu(&program, &nvidia()).expect("nothing to refuse");
        assert!(bodies.functions.is_empty());
    }

    #[test]
    fn the_cpu_path_leaves_reductions_to_the_llvm_backend() {
        let mut program = program("    val r = g.sum(1)");
        for item in &mut program.items {
            if let HirItem::Function(f) = item {
                f.target = HirTarget::Host;
            }
        }
        let bodies = lower_for_link(&program).expect("the CPU path lowers");
        assert!(bodies.functions.is_empty(), "{:?}", bodies.functions);
    }
}
