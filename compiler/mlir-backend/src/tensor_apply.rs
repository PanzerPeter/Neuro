// `.map(f)`, `.zip(other, f)` and `.reduce(init, f)` on the host: the body of a function
// outlined from a traversal, one `linalg.generic` that calls the function on each element.
//
// The function is the LLVM backend's code, a closure or a named function, so the body calls
// it through the function value the outlined function is handed last: a `{ fn, env }` pair,
// the environment passed ahead of the elements, as the LLVM backend calls one. The generic's
// loops run in row-major order, the order the LLVM backend's loop walked, so a function with
// side effects sees the elements in the same order. `.map` and `.zip` write each answer at
// the element's position; `.reduce` folds every element into a one-element tensor seeded
// with `init`, its index space a parallel axis of extent 1 and then every source axis.
//
// Nothing here is checked: the function carries its own checks, in the LLVM backend's code.

use crate::{
    errors::MlirError,
    guards::Guard,
    lower::map_type,
    tensor_arithmetic::{
        Generic, OperandAxes, empty_tensor, fill_block, indexing_maps, iterator_types, read_type,
        tensor_parts,
    },
    tensor_reduce::{append, apply, yield_value},
};

use melior::{
    Context,
    dialect::{func, llvm},
    ir::{
        Block, BlockLike, Identifier, Location, Region, RegionLike, Type, Value,
        attribute::{DenseI32ArrayAttribute, DenseI64ArrayAttribute},
        operation::OperationBuilder,
    },
};
use neuro_hir::{HirExpr, HirExprKind, HirFunction, HirStmt, HirTensorApply, HirType};

/// Build the body of `function` when it is one traversal, boxed in a one-element tensor for
/// a `.reduce`; `None` for any other body.
pub(crate) fn build_traversal<'c>(
    context: &'c Context,
    location: Location<'c>,
    function: &HirFunction,
) -> Result<Option<(Region<'c>, Vec<Guard>)>, MlirError> {
    let [HirStmt::Expr(body)] = function.body.as_slice() else {
        return Ok(None);
    };
    let (apply_expr, boxed) = match &body.kind {
        HirExprKind::TensorLiteral { elements } => match elements.as_slice() {
            [inner] => (inner, true),
            _ => return Ok(None),
        },
        _ => (body, false),
    };
    let HirExprKind::TensorApply {
        kind,
        receiver,
        operand,
        ..
    } = &apply_expr.kind
    else {
        return Ok(None);
    };
    if boxed != (*kind == HirTensorApply::Reduce) {
        return Ok(None);
    }
    // The function value is the one parameter of function type, passed last.
    let Some((callee, HirType::Function { ret, .. })) = function
        .params
        .iter()
        .enumerate()
        .rev()
        .find(|(_, param)| matches!(param.ty, HirType::Function { .. }))
        .map(|(index, param)| (index, &param.ty))
    else {
        return Ok(None);
    };
    let Some((_, shape)) = tensor_parts(&receiver.ty) else {
        return Ok(None);
    };
    if shape.iter().any(Option::is_none) {
        return Ok(None);
    }

    let mut slots = Vec::with_capacity(function.params.len());
    for param in &function.params {
        slots.push((map_type(context, read_type(&param.ty))?, location));
    }
    let block = Block::new(&slots);
    let argument = |expr: &HirExpr| -> Result<Option<Value>, MlirError> {
        let HirExprKind::Variable(name) = &expr.kind else {
            return Ok(None);
        };
        match function.params.iter().position(|param| param.name == *name) {
            Some(index) => Ok(Some(block.argument(index)?.into())),
            None => Ok(None),
        }
    };
    let (Some(source), Some(pair)) = (argument(receiver)?, Some(block.argument(callee)?.into()))
    else {
        return Ok(None);
    };
    let other = match operand {
        Some(operand) => match argument(operand)? {
            Some(value) => Some(value),
            None => return Ok(None),
        },
        None => None,
    };
    let pointer = llvm::r#type::pointer(context, 0);
    let half = |index: i64| {
        append(
            &block,
            llvm::extract_value(
                context,
                pair,
                DenseI64ArrayAttribute::new(context, &[index]),
                pointer,
                location,
            ),
        )
    };
    let target = Target {
        function: half(0)?,
        environment: half(1)?,
        answer: map_type(context, ret)?,
    };

    let rank = shape.len();
    let result_type = map_type(context, &body.ty)?;
    let empty = append(&block, empty_tensor(location, result_type, &[])?)?;
    let result = match (kind, other) {
        (HirTensorApply::Reduce, Some(seed)) => {
            let space = rank + 1;
            let written: OperandAxes = Some(vec![Some(0)]);
            let seeded = apply(
                context,
                location,
                &block,
                Generic {
                    inputs: &[seed],
                    destination: empty,
                    indexing_maps: indexing_maps(context, 1, &[&None, &written])?,
                    iterators: iterator_types(context, 1, 0)?,
                },
                result_type,
                fill_block(location, target.answer)?,
            )?;
            let walked: OperandAxes = Some((1..space).map(Some).collect());
            let element = element_type(context, &receiver.ty)?;
            let step = Block::new(&[(element, location), (target.answer, location)]);
            let folded = call(
                context,
                &step,
                location,
                &target,
                &[step.argument(1)?.into(), step.argument(0)?.into()],
            )?;
            yield_value(&step, location, folded)?;
            apply(
                context,
                location,
                &block,
                Generic {
                    inputs: &[source],
                    destination: seeded,
                    indexing_maps: indexing_maps(context, space, &[&walked, &written])?,
                    iterators: iterator_types(context, space, rank)?,
                },
                result_type,
                step,
            )?
        }
        (HirTensorApply::Map | HirTensorApply::Zip, other) => {
            let mut inputs = vec![source];
            let mut types = vec![(element_type(context, &receiver.ty)?, location)];
            if let (Some(other), Some(operand)) = (other, operand) {
                inputs.push(other);
                types.push((element_type(context, &operand.ty)?, location));
            }
            let own: OperandAxes = Some((0..rank).map(Some).collect());
            let maps = vec![&own; inputs.len() + 1];
            types.push((target.answer, location));
            let step = Block::new(&types);
            let elements = (0..inputs.len())
                .map(|index| Ok(step.argument(index)?.into()))
                .collect::<Result<Vec<Value>, MlirError>>()?;
            let answer = call(context, &step, location, &target, &elements)?;
            yield_value(&step, location, answer)?;
            apply(
                context,
                location,
                &block,
                Generic {
                    inputs: &inputs,
                    destination: empty,
                    indexing_maps: indexing_maps(context, rank, &maps)?,
                    iterators: iterator_types(context, rank, 0)?,
                },
                result_type,
                step,
            )?
        }
        _ => return Ok(None),
    };
    block.append_operation(func::r#return(&[result], location));
    let region = Region::new();
    region.append_block(block);
    Ok(Some((region, Vec::new())))
}

/// The function a traversal calls, split out of its function value once.
struct Target<'c, 'a> {
    function: Value<'c, 'a>,
    environment: Value<'c, 'a>,
    answer: Type<'c>,
}

/// `target`'s function called on `arguments`, its environment ahead of them.
fn call<'c, 'a>(
    context: &'c Context,
    block: &'a Block<'c>,
    location: Location<'c>,
    target: &Target<'c, '_>,
    arguments: &[Value<'c, '_>],
) -> Result<Value<'c, 'a>, MlirError> {
    let mut operands = vec![target.function, target.environment];
    operands.extend_from_slice(arguments);
    append(
        block,
        OperationBuilder::new("llvm.call", location)
            .add_operands(&operands)
            .add_results(&[target.answer])
            .add_attributes(&[
                (
                    Identifier::new(context, "operandSegmentSizes"),
                    DenseI32ArrayAttribute::new(context, &[operands.len() as i32, 0]).into(),
                ),
                (
                    Identifier::new(context, "op_bundle_sizes"),
                    DenseI32ArrayAttribute::new(context, &[]).into(),
                ),
            ])
            .build()?,
    )
}

/// The element type of a tensor, owned or borrowed.
fn element_type<'c>(context: &'c Context, tensor: &HirType) -> Result<Type<'c>, MlirError> {
    match tensor_parts(tensor) {
        Some((element, _)) => map_type(context, element),
        None => Err(MlirError::ModuleVerificationFailed),
    }
}
