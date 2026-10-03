// `place OP= value` on the host: the body of a function outlined from a compound assignment,
// one `linalg.generic` that reads the value and writes every element of the target in place.
//
// The body works on buffers rather than tensors. A tensor is a value, so writing one in place
// would mean trusting bufferization not to copy it; a `memref` is the buffer the `&mut`
// parameter names, and the generic's destination is that buffer itself. The target keeps the
// buffer it had, as the statement promises. The value is read with the host's trailing-axis
// broadcast, a scalar at every element, and each element is computed with the host's checks,
// the target's element on the left.

use crate::{
    errors::MlirError,
    guards::{At, Element, Guard, Lowering, MAX_SITES, Overflow, Side},
    lower::map_type,
    tensor_arithmetic::{
        OperandAxes, broadcast_axes, indexing_maps, iterator_types, read_type, row_major,
        tensor_parts,
    },
    tensor_reduce::yield_value,
};

use ast_types::BinaryOp;
use melior::{
    Context,
    dialect::func,
    ir::{
        Block, BlockLike, Identifier, Location, Region, RegionLike, Type, Value,
        attribute::DenseI32ArrayAttribute, operation::OperationBuilder, r#type::MemRefType,
    },
};
use neuro_hir::{HirExprKind, HirFunction, HirPlace, HirStmt, HirType};

/// Build the body of `function` when it is one compound assignment through its `&mut`
/// parameter; `None` for any other body.
pub(crate) fn build_compound<'c>(
    context: &'c Context,
    location: Location<'c>,
    function: &HirFunction,
    overflow: Overflow,
) -> Result<Option<(Region<'c>, Vec<Guard>)>, MlirError> {
    let [
        HirStmt::TensorCompoundAssign {
            place: HirPlace::Deref { pointer, .. },
            op,
            value,
            ty,
            span,
        },
    ] = function.body.as_slice()
    else {
        return Ok(None);
    };
    let (HirExprKind::Variable(target), HirExprKind::Variable(source)) =
        (&pointer.kind, &value.kind)
    else {
        return Ok(None);
    };
    let Some((element, shape)) = tensor_parts(ty) else {
        return Ok(None);
    };
    let (Some(kind), Some(axes)) = (
        Element::computed(element),
        broadcast_axes(&value.ty, element, shape),
    ) else {
        return Ok(None);
    };
    if !matches!(
        op,
        BinaryOp::Add
            | BinaryOp::Subtract
            | BinaryOp::Multiply
            | BinaryOp::Divide
            | BinaryOp::Modulo
    ) || shape.iter().any(Option::is_none)
    {
        return Ok(None);
    }
    let position = |name: &String| function.params.iter().position(|param| param.name == *name);
    let (Some(target), Some(source)) = (position(target), position(source)) else {
        return Ok(None);
    };

    let mut slots = Vec::with_capacity(function.params.len());
    for param in &function.params {
        slots.push((buffer_type(context, &param.ty)?, location));
    }
    let block = Block::new(&slots);
    let sites = {
        let lowering = Lowering::new(
            context,
            location,
            &block,
            (function.target, Side::Host),
            overflow,
        );
        let element_type = map_type(context, element)?;
        let body = Block::new(&[(element_type, location), (element_type, location)]);
        let at = At {
            offset: span.start,
            position: match (op, kind) {
                (
                    BinaryOp::Divide | BinaryOp::Modulo,
                    Element::Signed(_) | Element::Unsigned(_),
                ) => row_major(context, location, &body, shape)?,
                _ => None,
            },
        };
        let operands = (body.argument(1)?.into(), body.argument(0)?.into());
        let updated = lowering
            .arith(&body, *op, kind, operands, at)?
            .ok_or(MlirError::ModuleVerificationFailed)?;
        yield_value(&body, location, updated)?;

        let rank = shape.len();
        let written: OperandAxes = Some((0..rank).map(Some).collect());
        let region = Region::new();
        region.append_block(body);
        let inputs: Value = block.argument(source)?.into();
        let destination: Value = block.argument(target)?.into();
        block.append_operation(
            OperationBuilder::new("linalg.generic", location)
                .add_operands(&[inputs, destination])
                .add_attributes(&[
                    (
                        Identifier::new(context, "indexing_maps"),
                        indexing_maps(context, rank, &[&axes, &written])?,
                    ),
                    (
                        Identifier::new(context, "iterator_types"),
                        iterator_types(context, rank, 0)?,
                    ),
                    (
                        Identifier::new(context, "operandSegmentSizes"),
                        DenseI32ArrayAttribute::new(context, &[1, 1]).into(),
                    ),
                ])
                .add_regions([region])
                .build()?,
        );
        lowering.into_sites()
    };
    if sites.len() > MAX_SITES {
        return Ok(None);
    }
    block.append_operation(func::r#return(&[], location));
    let region = Region::new();
    region.append_block(block);
    Ok(Some((region, sites)))
}

/// A tensor parameter, owned or borrowed, as the buffer it is; a scalar as itself.
fn buffer_type<'c>(context: &'c Context, ty: &HirType) -> Result<Type<'c>, MlirError> {
    let tensor = match ty {
        HirType::Reference { inner, .. } => inner.as_ref(),
        other => read_type(other),
    };
    let HirType::Tensor { element, shape, .. } = tensor else {
        return map_type(context, ty);
    };
    let extents: Vec<i64> = shape
        .iter()
        .map(|extent| extent.map_or(i64::MIN, |extent| extent as i64))
        .collect();
    Ok(MemRefType::new(map_type(context, element)?, &extents, None, None).into())
}
