// Slices and permuting shape casts as `linalg`, for GPU bodies: each is a copy into a fresh
// tensor, as on the host, where a slice is never a view.
//
// A permutation is a `linalg.generic` whose input map sends each receiver axis to the result
// dimension it became. A slice reads its source at an index the body computes from the result
// position (`linalg.index`): a range axis walks `start + d * step` (back from `end - 1` when
// reversed), and a position axis reads one fixed index. A position is either a literal in
// range, or, in a function outlined to follow its operands, a parameter the call site has
// already checked against its extent with the host's own guard; a GPU body cannot stop the
// program, so it never reads a position nothing checked.

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
    dialect::arith,
    ir::{
        Block, Identifier, Location, Operation, Type, Value, attribute::IntegerAttribute,
        operation::OperationBuilder, r#type::IntegerType,
    },
};
use neuro_hir::{HirExpr, HirExprKind, HirTarget, HirTensorAxis, HirType};
use shared_types::Literal;

/// The width of the integer attribute `linalg.index` names its dimension with.
const DIMENSION_BITS: u32 = 64;

/// How a slice reads one source axis.
enum Read<'c, 'a> {
    /// Along result dimension `dimension`: `first + d * stride`.
    Walk {
        dimension: usize,
        first: i64,
        stride: i64,
    },
    /// At one index, the same at every point.
    At(Value<'c, 'a>),
}

/// Lower `t[...]` with at least one axis surviving into a fresh tensor of the slice's type.
pub(crate) fn build_slice<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    block: &'a Block<'c>,
    slice: &HirExpr,
    scope: &[(String, Value<'c, 'a>)],
    target: HirTarget,
) -> Result<Option<Value<'c, 'a>>, MlirError> {
    let HirExprKind::TensorIndex { object, axes } = &slice.kind else {
        return Ok(None);
    };
    let (Some((element, result_shape)), Some((_, source_shape))) =
        (tensor_parts(&slice.ty), tensor_parts(&object.ty))
    else {
        return Ok(None);
    };
    if axes.len() != source_shape.len()
        || result_shape.iter().chain(source_shape).any(Option::is_none)
    {
        return Ok(None);
    }
    let Some(source) = build_expression(context, location, block, object, scope, target)? else {
        return Ok(None);
    };

    let index = Type::index(context);
    let mut reads = Vec::with_capacity(axes.len());
    let mut dimension = 0;
    for (axis, extent) in axes.iter().zip(source_shape.iter().flatten()) {
        let read = match axis {
            HirTensorAxis::Range {
                start,
                end,
                reversed,
                step,
            } => {
                let (Ok(start), Ok(end), Ok(step)) = (
                    i64::try_from(*start),
                    i64::try_from(*end),
                    i64::try_from(*step),
                ) else {
                    return Ok(None);
                };
                dimension += 1;
                match reversed {
                    true => Read::Walk {
                        dimension: dimension - 1,
                        first: end - 1,
                        stride: -step,
                    },
                    false => Read::Walk {
                        dimension: dimension - 1,
                        first: start,
                        stride: step,
                    },
                }
            }
            HirTensorAxis::Position(position) => {
                let Some(at) =
                    position_index(context, location, block, position, *extent, scope, target)?
                else {
                    return Ok(None);
                };
                Read::At(at)
            }
        };
        reads.push(read);
    }
    if dimension != result_shape.len() {
        return Ok(None);
    }

    let tensor_type = map_type(context, &slice.ty)?;
    let element_type = map_type(context, element)?;
    let rank = result_shape.len();
    let own: OperandAxes = Some((0..rank).map(Some).collect());
    let empty = append(block, empty_tensor(location, tensor_type, &[])?)?;

    let body = Block::new(&[(element_type, location)]);
    let mut indices = Vec::with_capacity(reads.len());
    for read in &reads {
        indices.push(match read {
            Read::At(at) => *at,
            Read::Walk {
                dimension,
                first,
                stride,
            } => walk(context, location, &body, index, *dimension, *first, *stride)?,
        });
    }
    let mut operands = vec![source];
    operands.extend(indices);
    let value = append(
        &body,
        OperationBuilder::new("tensor.extract", location)
            .add_operands(&operands)
            .add_results(&[element_type])
            .build()?,
    )?;
    yield_value(&body, location, value)?;

    Ok(Some(apply(
        context,
        location,
        block,
        Generic {
            inputs: &[],
            destination: empty,
            indexing_maps: indexing_maps(context, rank, &[&own])?,
            iterators: iterator_types(context, rank, 0)?,
        },
        tensor_type,
        body,
    )?))
}

/// One position axis as an `index`: a literal inside the axis, or a parameter of a function
/// outlined to follow its operands, whose call site has checked it. Anything else would be an
/// unchecked read on a GPU, so it leaves the slice to the LLVM backend.
fn position_index<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    block: &'a Block<'c>,
    position: &HirExpr,
    extent: usize,
    scope: &[(String, Value<'c, 'a>)],
    target: HirTarget,
) -> Result<Option<Value<'c, 'a>>, MlirError> {
    let index = Type::index(context);
    match &position.kind {
        HirExprKind::Literal(Literal::Integer(value, _))
            if usize::try_from(*value).is_ok_and(|value| value < extent) =>
        {
            Ok(Some(append(
                block,
                arith::constant(
                    context,
                    IntegerAttribute::new(index, *value as i64).into(),
                    location,
                ),
            )?))
        }
        HirExprKind::Variable(_) if target == HirTarget::FollowsOperands => {
            let Some(value) = build_expression(context, location, block, position, scope, target)?
            else {
                return Ok(None);
            };
            let cast = match position.ty {
                HirType::U8 | HirType::U16 | HirType::U32 | HirType::U64 => "arith.index_castui",
                HirType::I8 | HirType::I16 | HirType::I32 | HirType::I64 => "arith.index_cast",
                _ => return Ok(None),
            };
            Ok(Some(append(
                block,
                OperationBuilder::new(cast, location)
                    .add_operands(&[value])
                    .add_results(&[index])
                    .build()?,
            )?))
        }
        _ => Ok(None),
    }
}

/// `first + d * stride` for the index-space dimension `dimension`, inside a body.
fn walk<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    body: &'a Block<'c>,
    index: Type<'c>,
    dimension: usize,
    first: i64,
    stride: i64,
) -> Result<Value<'c, 'a>, MlirError> {
    let at = append(body, linalg_index(context, location, dimension)?)?;
    if first == 0 && stride == 1 {
        return Ok(at);
    }
    let constant = |value: i64| {
        append(
            body,
            arith::constant(
                context,
                IntegerAttribute::new(index, value).into(),
                location,
            ),
        )
    };
    let stride = constant(stride)?;
    let first = constant(first)?;
    let scaled = append(body, arith::muli(at, stride, location))?;
    append(body, arith::addi(first, scaled, location))
}

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

/// Lower a permuting shape cast (`.t()`, `.permute(...)`) into a fresh tensor whose axis `d`
/// walks receiver axis `permutation[d]`.
pub(crate) fn build_permute<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    block: &'a Block<'c>,
    cast: &HirExpr,
    scope: &[(String, Value<'c, 'a>)],
    target: HirTarget,
) -> Result<Option<Value<'c, 'a>>, MlirError> {
    let HirExprKind::TensorShapeCast {
        receiver,
        permutation: Some(permutation),
    } = &cast.kind
    else {
        return Ok(None);
    };
    let (Some((element, result_shape)), Some((_, source_shape))) =
        (tensor_parts(&cast.ty), tensor_parts(&receiver.ty))
    else {
        return Ok(None);
    };
    let rank = result_shape.len();
    if permutation.len() != rank
        || source_shape.len() != rank
        || result_shape.iter().any(Option::is_none)
    {
        return Ok(None);
    }
    let mut walked = vec![None; rank];
    for (dimension, &axis) in permutation.iter().enumerate() {
        let Some(slot) = walked.get_mut(axis) else {
            return Ok(None);
        };
        *slot = Some(dimension);
    }
    if walked.contains(&None) {
        return Ok(None);
    }
    let Some(source) = build_expression(context, location, block, receiver, scope, target)? else {
        return Ok(None);
    };

    let tensor_type = map_type(context, &cast.ty)?;
    let element_type = map_type(context, element)?;
    let own: OperandAxes = Some((0..rank).map(Some).collect());
    let read: OperandAxes = Some(walked);
    let empty = append(block, empty_tensor(location, tensor_type, &[])?)?;
    Ok(Some(apply(
        context,
        location,
        block,
        Generic {
            inputs: &[source],
            destination: empty,
            indexing_maps: indexing_maps(context, rank, &[&read, &own])?,
            iterators: iterator_types(context, rank, 0)?,
        },
        tensor_type,
        fill_block(location, element_type)?,
    )?))
}

#[cfg(test)]
mod tests {
    use crate::{GpuTarget, MlirError, lower_for_gpu};

    use neuro_hir::HirProgram;

    fn parse(source: &str) -> HirProgram {
        let ast = syntax_parsing::parse(source).expect("the program parses");
        hir_lowering::lower_program(&ast).expect("the program lowers to HIR")
    }

    fn program(body: &str) -> HirProgram {
        parse(&format!(
            "enum Device {{\n    CPU,\n    GPU(i32)\n}}\n\nfunc main() -> i32 {{\n    val m: Tensor<f32, [37, 19]> = Tensor::ones()\n    val g = m.clone().to(Device::GPU(0))\n{body}\n    return 0\n}}\n"
        ))
    }

    fn nvidia() -> GpuTarget {
        GpuTarget::Nvidia {
            chip: "sm_80".to_string(),
        }
    }

    fn launches(program: &HirProgram) -> usize {
        let bodies = lower_for_gpu(program, &nvidia()).expect("the body lowers");
        assert_eq!(bodies.functions.len(), 1, "one outlined body");
        bodies
            .llvm_ir
            .matches("call void @mgpuLaunchKernel")
            .count()
    }

    #[test]
    fn a_stepped_and_reversed_slice_is_one_gather() {
        assert_eq!(
            launches(&program("    val s = g[(3..30).step(4), (0..19).rev()]")),
            1
        );
    }

    #[test]
    fn a_checked_position_is_a_parameter_of_the_gather() {
        let program = program("    val k = 5u64\n    val s = g[k, 2..17]");
        let bodies = lower_for_gpu(&program, &nvidia()).expect("the body lowers");
        assert_eq!(bodies.functions.len(), 1);
        // The position crosses as itself, after the receiver's rank-2 descriptor.
        assert!(
            bodies.llvm_ir.contains("i64 %5, i64 %6, i64 %7"),
            "{}",
            bodies.llvm_ir
        );
    }

    #[test]
    fn a_permutation_is_one_copy() {
        assert_eq!(launches(&program("    val t = g.clone().t()")), 1);
    }

    #[test]
    fn a_gpu_body_slices_only_at_literal_positions() {
        let source = |body: &str| {
            format!(
                "@gpu\nfunc row(a: &Tensor<f32, [4, 3]>) -> Tensor<f32, [3]> {{\n{body}\n}}\n\nfunc main() -> i32 {{\n    val a = Tensor::<f32, [4, 3]>::ones()\n    val r = row(&a)\n    return 0\n}}\n"
            )
        };
        lower_for_gpu(&parse(&source("    a[2, ..]")), &nvidia())
            .expect("a literal position lowers");
        let refused = lower_for_gpu(&parse(&source("    val i = 2u64\n    a[i, ..]")), &nvidia());
        assert!(
            matches!(refused, Err(MlirError::GpuBodiesNotLowered(_))),
            "nothing checks a `@gpu` body's position: {refused:?}"
        );
    }
}
