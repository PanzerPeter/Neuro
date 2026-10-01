// `.sum()`, `.mean()`, `.max()` and `.min()` as `linalg`, for GPU bodies.
//
// A reduction is a `linalg.generic` whose index space is the result's axes followed by the
// reduced ones: the result axes are `parallel` and become GPU threads, the reduced ones are
// `reduction` and become a sequential loop inside each thread. So every result element is
// folded by one thread in the source's own order, which is the order the LLVM backend folds
// in, and the two give the same bits. A whole-tensor reduction writes a one-element tensor
// (its one parallel axis has extent 1), since a GPU body returns buffers only.
//
// ponytail: one thread per result element, so a whole-tensor reduction runs in one GPU
// thread. A tree reduction would use the whole GPU, once a ruling lets a device sum differ
// from the host's in its last bits.
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
    tensor_sort::precedes,
};

use melior::{
    Context,
    dialect::arith,
    ir::{
        Block, BlockLike, Location, Region, RegionLike, Type, Value, attribute::FloatAttribute,
        operation::OperationBuilder,
    },
};
use neuro_hir::{HirExpr, HirExprKind, HirReduceOp, HirTarget, HirType};

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

    let length = FloatAttribute::new(context, element_type, layout.length as f64).into();
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
        })
    }
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
    let descending = match op {
        HirReduceOp::Sum | HirReduceOp::Mean => {
            let sum = append(&block, arith::addf(carried, value, location))?;
            yield_value(&block, location, sum)?;
            return Ok(block);
        }
        HirReduceOp::Max => true,
        HirReduceOp::Min => false,
    };
    let wins = precedes(context, location, &block, value, carried, descending)?;
    let kept = append(&block, arith::select(wins, value, carried, location))?;
    yield_value(&block, location, kept)?;
    Ok(block)
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
