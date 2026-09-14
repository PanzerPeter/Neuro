use crate::{errors::MlirError, lower::map_type};

use ast_types::BinaryOp;
use melior::{
    dialect::{arith, func},
    ir::{
        attribute::{DenseI32ArrayAttribute, FloatAttribute, IntegerAttribute},
        operation::OperationBuilder,
        Attribute, Block, BlockLike, Identifier, Location, Operation, Region, RegionLike, Type,
        Value,
    },
    Context,
};
use neuro_hir::{HirExpr, HirExprKind, HirFunction, HirStmt, HirType};

/// How many region arguments `linalg.generic` passes an element-wise binary body:
/// one per operand, the destination's included.
const ELEMENTWISE_BODY_ARGUMENTS: usize = 3;

/// How many region arguments the zero-fill `linalg.generic` passes: the scalar it
/// writes, then the destination slot it writes over.
const FILL_BODY_ARGUMENTS: usize = 2;

/// The index-space rank of a matrix product: two parallel axes over the result and
/// one reduction axis over the contracted extent.
const CONTRACTION_RANK: usize = 3;

/// The only extent that may be stretched across a larger result axis. Any other
/// mismatch is a shape error the frontend owns, not something to lower.
const BROADCAST_EXTENT: usize = 1;

/// Where a stretched axis reads: element `0`, at every point of the result axis
/// it covers. Written into the operand's affine map in place of a dimension.
const STRETCHED_INDEX: &str = "0";

/// The `arith` operation that carries one element of an element-wise tensor
/// operation. A function pointer rather than an enum because every candidate
/// already has this exact shape in `melior`.
type ScalarOp = for<'c, 'a> fn(Value<'c, 'a>, Value<'c, 'a>, Location<'c>) -> Operation<'c>;

/// How one operand is read at each point of the result's index space.
///
/// `None` is a scalar operand: it has no index space, so its affine map has no
/// results and `linalg.generic` hands the same value to every point. `Some(axes)`
/// is a tensor, one entry per axis of that operand: the result axis it walks, or
/// `None` where a size-1 extent is stretched and the operand is read at index 0.
type OperandAxes = Option<Vec<Option<usize>>>;

/// Build the body region of a function whose statements are all tensor arithmetic.
///
/// `Ok(None)` is an ordinary answer rather than a failure: this path lowers the
/// element-wise arithmetic the language defines on tensors and nothing else,
/// because 2C's own decision keeps every scalar and every 2B tensor operation on
/// the inkwell backend permanently. A body it cannot express leaves its function
/// as the external declaration it already was, which is why the caller treats
/// `None` as "declare".
pub(crate) fn build_body<'c>(
    context: &'c Context,
    location: Location<'c>,
    function: &HirFunction,
) -> Result<Option<Region<'c>>, MlirError> {
    // Cheap filter first: a function that does not hand a tensor back cannot be
    // one of these, and every scalar function in the program hits it.
    if tensor_parts(&function.return_type).is_none() {
        return Ok(None);
    }

    let mut slots = Vec::with_capacity(function.params.len());
    for param in &function.params {
        slots.push((map_type(context, &param.ty)?, location));
    }

    let block = Block::new(&slots);

    let built = {
        let mut scope: Vec<(String, Value<'c, '_>)> = Vec::with_capacity(function.params.len());
        for (index, param) in function.params.iter().enumerate() {
            scope.push((param.name.clone(), block.argument(index)?.into()));
        }
        build_statements(context, location, &block, &function.body, &mut scope)?
    };

    if !built {
        return Ok(None);
    }

    let region = Region::new();
    region.append_block(block);

    Ok(Some(region))
}

/// A tensor's element type and its shape, `None` for every other type.
fn tensor_parts(ty: &HirType) -> Option<(&HirType, &[Option<usize>])> {
    let HirType::Tensor { element, shape, .. } = ty else {
        return None;
    };

    Some((element.as_ref(), shape.as_slice()))
}

/// Append the statements to `block`, reporting whether all of them were expressible.
fn build_statements<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    block: &'a Block<'c>,
    statements: &[HirStmt],
    scope: &mut Vec<(String, Value<'c, 'a>)>,
) -> Result<bool, MlirError> {
    let Some((last, leading)) = statements.split_last() else {
        return Ok(false);
    };

    for statement in leading {
        let HirStmt::VarDecl {
            name,
            init: Some(init),
            ..
        } = statement
        else {
            return Ok(false);
        };
        let Some(value) = build_expression(context, location, block, init, scope)? else {
            return Ok(false);
        };
        scope.push((name.clone(), value));
    }

    // The body must end in a `return`: this path produces a value, and a function
    // that falls off its end has none to hand back.
    let HirStmt::Return {
        value: Some(value), ..
    } = last
    else {
        return Ok(false);
    };
    let Some(result) = build_expression(context, location, block, value, scope)? else {
        return Ok(false);
    };
    block.append_operation(func::r#return(&[result], location));

    Ok(true)
}

/// Lower one expression, yielding the SSA value it produces.
fn build_expression<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    block: &'a Block<'c>,
    expression: &HirExpr,
    scope: &[(String, Value<'c, 'a>)],
) -> Result<Option<Value<'c, 'a>>, MlirError> {
    match &expression.kind {
        // Searched from the back so a shadowing binding wins over the one it hides.
        HirExprKind::Variable(name) => Ok(scope
            .iter()
            .rev()
            .find(|(bound, _)| bound == name)
            .map(|(_, value)| *value)),
        // `@` contracts an axis instead of walking the result element for element, so it
        // is a different index space rather than a different body.
        HirExprKind::Binary {
            op: BinaryOp::MatMul,
            ..
        } => build_matmul(context, location, block, expression, scope),
        HirExprKind::Binary { .. } => {
            build_elementwise(context, location, block, expression, scope)
        }
        _ => Ok(None),
    }
}

/// Lower `a OP b` over two tensors into `tensor.empty` plus a `linalg.generic`.
fn build_elementwise<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    block: &'a Block<'c>,
    expression: &HirExpr,
    scope: &[(String, Value<'c, 'a>)],
) -> Result<Option<Value<'c, 'a>>, MlirError> {
    let HirExprKind::Binary { op, left, right } = &expression.kind else {
        return Ok(None);
    };
    let Some((element, result_shape)) = tensor_parts(&expression.ty) else {
        return Ok(None);
    };
    let Some(scalar) = scalar_op(*op, element) else {
        return Ok(None);
    };
    let Some(axes) = [&left.ty, &right.ty]
        .into_iter()
        .map(|operand| broadcast_axes(operand, element, result_shape))
        .collect::<Option<Vec<_>>>()
    else {
        return Ok(None);
    };
    let Some(lhs) = build_expression(context, location, block, left, scope)? else {
        return Ok(None);
    };
    let Some(rhs) = build_expression(context, location, block, right, scope)? else {
        return Ok(None);
    };
    let Some(sizes) = dynamic_sizes(context, location, block, result_shape, &[lhs, rhs], &axes)?
    else {
        return Ok(None);
    };

    let tensor_type = map_type(context, &expression.ty)?;
    let element_type = map_type(context, element)?;
    let rank = result_shape.len();

    let destination = block
        .append_operation(empty_tensor(location, tensor_type, &sizes)?)
        .result(0)?
        .into();

    // The destination walks every result axis in order; that identity map is what
    // makes the operation element-wise rather than a gather.
    let destination_axes = Some((0..rank).map(Some).collect());
    let body = Region::new();
    body.append_block(scalar_body(location, element_type, scalar)?);
    let generic = generic_op(
        context,
        location,
        Generic {
            inputs: &[lhs, rhs],
            destination,
            indexing_maps: indexing_maps(context, rank, &[&axes[0], &axes[1], &destination_axes])?,
            iterators: iterator_types(context, rank, 0)?,
        },
        tensor_type,
        body,
    )?;

    Ok(Some(block.append_operation(generic).result(0)?.into()))
}

/// Lower `a @ b` over two rank-2 tensors into the canonical `linalg` matrix product:
/// a `tensor.empty` destination, a `linalg.generic` that fills it with the element's
/// zero, and a second one that accumulates the product into it.
///
/// The fill is not optional. A reduction READS its destination at every point — that is
/// what makes it an accumulator — and `tensor.empty` is undefined memory, so the sum
/// would start from whatever the allocator last left there.
fn build_matmul<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    block: &'a Block<'c>,
    expression: &HirExpr,
    scope: &[(String, Value<'c, 'a>)],
) -> Result<Option<Value<'c, 'a>>, MlirError> {
    let HirExprKind::Binary { left, right, .. } = &expression.kind else {
        return Ok(None);
    };
    let Some((element, result_shape)) = tensor_parts(&expression.ty) else {
        return Ok(None);
    };
    let (Some(multiply), Some(add)) = (
        scalar_op(BinaryOp::Multiply, element),
        scalar_op(BinaryOp::Add, element),
    ) else {
        return Ok(None);
    };
    let Some(zero) = zero_attribute(context, element, map_type(context, element)?) else {
        return Ok(None);
    };
    // Every extent must be static here. The destination's two are what `tensor.empty`
    // is sized with, and the contracted one bounds the reduction; a `?` supplies none of
    // them, and a `tensor.dim` cannot recover an axis the result does not have.
    if !contraction_is_static(left, right, result_shape) {
        return Ok(None);
    }
    let Some(lhs) = build_expression(context, location, block, left, scope)? else {
        return Ok(None);
    };
    let Some(rhs) = build_expression(context, location, block, right, scope)? else {
        return Ok(None);
    };

    let tensor_type = map_type(context, &expression.ty)?;
    let element_type = map_type(context, element)?;
    let rank = result_shape.len();

    let empty = block
        .append_operation(empty_tensor(location, tensor_type, &[])?)
        .result(0)?
        .into();
    let identity = block
        .append_operation(arith::constant(context, zero, location))
        .result(0)?
        .into();
    let fill_body = Region::new();
    fill_body.append_block(fill_block(location, element_type)?);
    let destination_axes: OperandAxes = Some((0..rank).map(Some).collect());
    let filled = block
        .append_operation(generic_op(
            context,
            location,
            Generic {
                inputs: &[identity],
                destination: empty,
                indexing_maps: indexing_maps(context, rank, &[&None, &destination_axes])?,
                iterators: iterator_types(context, rank, 0)?,
            },
            tensor_type,
            fill_body,
        )?)
        .result(0)?
        .into();

    // `(d0, d1, d2)` is (row, column, contracted), so the left operand reads
    // `(d0, d2)`, the right `(d2, d1)`, and the accumulator the result's own `(d0, d1)`.
    let left_axes: OperandAxes = Some(vec![Some(0), Some(2)]);
    let right_axes: OperandAxes = Some(vec![Some(2), Some(1)]);
    let accumulator_axes: OperandAxes = Some(vec![Some(0), Some(1)]);
    let body = Region::new();
    body.append_block(contraction_block(location, element_type, multiply, add)?);

    Ok(Some(
        block
            .append_operation(generic_op(
                context,
                location,
                Generic {
                    inputs: &[lhs, rhs],
                    destination: filled,
                    indexing_maps: indexing_maps(
                        context,
                        CONTRACTION_RANK,
                        &[&left_axes, &right_axes, &accumulator_axes],
                    )?,
                    iterators: iterator_types(context, CONTRACTION_RANK, 1)?,
                },
                tensor_type,
                body,
            )?)
            .result(0)?
            .into(),
    ))
}

/// Whether the three shapes really are the static `[M, K] @ [K, N] -> [M, N]` this
/// path emits: two rank-2 operands agreeing on the contracted axis, and no `?` anywhere.
///
/// The frontend has already agreed all of that; it is re-derived here because a stage
/// carries the rule it needs over the shared type rather than importing a sibling's,
/// and because nothing in the compiler reaches this path from a compile yet.
fn contraction_is_static(left: &HirExpr, right: &HirExpr, result: &[Option<usize>]) -> bool {
    let (Some((_, left_shape)), Some((_, right_shape))) =
        (tensor_parts(&left.ty), tensor_parts(&right.ty))
    else {
        return false;
    };
    let ([rows, columns], [left_rows, contracted], [contracted_again, right_columns]) =
        (result, left_shape, right_shape)
    else {
        return false;
    };
    left_rows == rows
        && right_columns == columns
        && contracted == contracted_again
        && [rows, columns, contracted]
            .iter()
            .all(|extent| extent.is_some())
}

/// The additive identity of `element`, as the attribute an `arith.constant` carries.
/// `None` for an element the language gives no arithmetic, which is the same set
/// `scalar_op` refuses.
fn zero_attribute<'c>(
    context: &'c Context,
    element: &HirType,
    element_type: Type<'c>,
) -> Option<Attribute<'c>> {
    match element {
        HirType::F32 | HirType::F64 => Some(FloatAttribute::new(context, element_type, 0.0).into()),
        HirType::I8
        | HirType::I16
        | HirType::I32
        | HirType::I64
        | HirType::U8
        | HirType::U16
        | HirType::U32
        | HirType::U64 => Some(IntegerAttribute::new(element_type, 0).into()),
        _ => None,
    }
}

/// The zero-fill body: hand the scalar input straight through to the destination slot.
fn fill_block<'c>(location: Location<'c>, element: Type<'c>) -> Result<Block<'c>, MlirError> {
    let block = Block::new(&[(element, location); FILL_BODY_ARGUMENTS]);
    let value = block.argument(0)?.into();

    block.append_operation(
        OperationBuilder::new("linalg.yield", location)
            .add_operands(&[value])
            .build()?,
    );

    Ok(block)
}

/// The matrix-product body: multiply the two operand elements and add the product to
/// the accumulator the destination already carries.
fn contraction_block<'c>(
    location: Location<'c>,
    element: Type<'c>,
    multiply: ScalarOp,
    add: ScalarOp,
) -> Result<Block<'c>, MlirError> {
    let block = Block::new(&[(element, location); ELEMENTWISE_BODY_ARGUMENTS]);

    let product = block
        .append_operation(multiply(
            block.argument(0)?.into(),
            block.argument(1)?.into(),
            location,
        ))
        .result(0)?
        .into();
    let summed = block
        .append_operation(add(block.argument(2)?.into(), product, location))
        .result(0)?
        .into();

    block.append_operation(
        OperationBuilder::new("linalg.yield", location)
            .add_operands(&[summed])
            .build()?,
    );

    Ok(block)
}

/// How an operand participates in a result of shape `result`, or `None` where it
/// cannot: a stretched extent other than 1, a rank above the result's, an extent
/// that cannot be proven equal to a `?`, or a different element type.
///
/// Shapes align at their **trailing** axis, so an operand of lower rank supplies
/// the innermost axes and is repeated across the leading ones.
fn broadcast_axes(
    operand: &HirType,
    element: &HirType,
    result: &[Option<usize>],
) -> Option<OperandAxes> {
    if operand == element {
        return Some(None);
    }

    let (operand_element, shape) = tensor_parts(operand)?;
    if operand_element != element {
        return None;
    }
    let offset = result.len().checked_sub(shape.len())?;

    shape
        .iter()
        .enumerate()
        .map(|(axis, extent)| match extent {
            matched if *matched == result[offset + axis] => Some(Some(offset + axis)),
            // A `?` operand extent is never stretched: nothing here can prove it
            // is 1, and guessing wrong would silently read the wrong element.
            Some(BROADCAST_EXTENT) => Some(None),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()
        .map(Some)
}

/// One `index` value per dynamic result axis, sizing the `tensor.empty` the
/// result is written into. `None` where no operand walks such an axis, leaving
/// its extent unknowable at the destination.
fn dynamic_sizes<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    block: &'a Block<'c>,
    result: &[Option<usize>],
    values: &[Value<'c, 'a>; 2],
    axes: &[OperandAxes],
) -> Result<Option<Vec<Value<'c, 'a>>>, MlirError> {
    let index_type = Type::index(context);
    let mut sizes = Vec::new();

    for (axis, extent) in result.iter().enumerate() {
        if extent.is_some() {
            continue;
        }
        // Only an operand that walks the axis carries its extent; a stretched
        // operand is size 1 there and says nothing about the result.
        let Some((value, position)) = values.iter().zip(axes).find_map(|(value, axes)| {
            let position = axes
                .as_ref()?
                .iter()
                .position(|walked| *walked == Some(axis))?;
            Some((*value, position))
        }) else {
            return Ok(None);
        };

        let position = block
            .append_operation(arith::constant(
                context,
                IntegerAttribute::new(index_type, position as i64).into(),
                location,
            ))
            .result(0)?
            .into();
        sizes.push(
            block
                .append_operation(dim_op(location, value, position, index_type)?)
                .result(0)?
                .into(),
        );
    }

    Ok(Some(sizes))
}

/// What one `linalg.generic` needs beyond its types: the inputs it reads, the
/// destination it writes into, and the two attributes that say how each is walked.
struct Generic<'c, 'a> {
    inputs: &'a [Value<'c, 'a>],
    destination: Value<'c, 'a>,
    indexing_maps: Attribute<'c>,
    iterators: Attribute<'c>,
}

/// Assemble one `linalg.generic`, with `body` as its region.
///
/// Every shape this crate emits goes through here — the element-wise operation, the
/// zero fill, and the matrix product — because the three differ only in their maps,
/// their iterators and their body, which are exactly the arguments.
fn generic_op<'c>(
    context: &'c Context,
    location: Location<'c>,
    generic: Generic<'c, '_>,
    tensor_type: Type<'c>,
    body: Region<'c>,
) -> Result<Operation<'c>, MlirError> {
    let mut operands = generic.inputs.to_vec();
    operands.push(generic.destination);
    // `linalg.generic` splits its operands into the inputs it reads and the
    // destinations it writes; there is exactly one of the latter here.
    let segments = [generic.inputs.len() as i32, 1];

    Ok(OperationBuilder::new("linalg.generic", location)
        .add_operands(&operands)
        .add_results(&[tensor_type])
        .add_attributes(&[
            (
                Identifier::new(context, "indexing_maps"),
                generic.indexing_maps,
            ),
            (
                Identifier::new(context, "iterator_types"),
                generic.iterators,
            ),
            (
                Identifier::new(context, "operandSegmentSizes"),
                DenseI32ArrayAttribute::new(context, &segments).into(),
            ),
        ])
        .add_regions([body])
        .build()?)
}

/// The `linalg.generic` body: apply `scalar` to the two input elements and yield it.
fn scalar_body<'c>(
    location: Location<'c>,
    element: Type<'c>,
    scalar: ScalarOp,
) -> Result<Block<'c>, MlirError> {
    // The region takes one argument per operand, so the third is whatever the
    // destination already holds. An element-wise write overwrites it unread.
    let block = Block::new(&[(element, location); ELEMENTWISE_BODY_ARGUMENTS]);

    let lhs = block.argument(0)?.into();
    let rhs = block.argument(1)?.into();
    let value = block
        .append_operation(scalar(lhs, rhs, location))
        .result(0)?
        .into();

    block.append_operation(
        OperationBuilder::new("linalg.yield", location)
            .add_operands(&[value])
            .build()?,
    );

    Ok(block)
}

/// The destination `linalg.generic` writes its result into. `sizes` carries one
/// extent per dynamic axis, in shape order, which is what `tensor.empty` expects.
fn empty_tensor<'c>(
    location: Location<'c>,
    tensor: Type<'c>,
    sizes: &[Value<'c, '_>],
) -> Result<Operation<'c>, MlirError> {
    Ok(OperationBuilder::new("tensor.empty", location)
        .add_operands(sizes)
        .add_results(&[tensor])
        .build()?)
}

/// The run-time extent of one axis of an operand, read back off the value itself.
fn dim_op<'c>(
    location: Location<'c>,
    tensor: Value<'c, '_>,
    axis: Value<'c, '_>,
    index: Type<'c>,
) -> Result<Operation<'c>, MlirError> {
    Ok(OperationBuilder::new("tensor.dim", location)
        .add_operands(&[tensor, axis])
        .add_results(&[index])
        .build()?)
}

/// Which `arith` operation carries one element, or `None` where the language
/// defines no such arithmetic on tensors.
///
/// `f16` / `bf16` are absent deliberately: the HIR contract gives them a narrow
/// scalar role with no arithmetic, so a tensor of them has none either. Integer
/// division is the one place signedness changes the operation rather than only
/// the type.
fn scalar_op(op: BinaryOp, element: &HirType) -> Option<ScalarOp> {
    match element {
        HirType::F32 | HirType::F64 => match op {
            BinaryOp::Add => Some(arith::addf),
            BinaryOp::Subtract => Some(arith::subf),
            BinaryOp::Multiply => Some(arith::mulf),
            BinaryOp::Divide => Some(arith::divf),
            _ => None,
        },
        HirType::I8 | HirType::I16 | HirType::I32 | HirType::I64 => match op {
            BinaryOp::Add => Some(arith::addi),
            BinaryOp::Subtract => Some(arith::subi),
            BinaryOp::Multiply => Some(arith::muli),
            BinaryOp::Divide => Some(arith::divsi),
            _ => None,
        },
        HirType::U8 | HirType::U16 | HirType::U32 | HirType::U64 => match op {
            BinaryOp::Add => Some(arith::addi),
            BinaryOp::Subtract => Some(arith::subi),
            BinaryOp::Multiply => Some(arith::muli),
            BinaryOp::Divide => Some(arith::divui),
            _ => None,
        },
        _ => None,
    }
}

/// One affine map per `linalg.generic` operand, in operand order, saying where
/// that operand is read at each point of the result's index space.
fn indexing_maps<'c>(
    context: &'c Context,
    rank: usize,
    operands: &[&OperandAxes],
) -> Result<Attribute<'c>, MlirError> {
    let maps = operands
        .iter()
        .map(|axes| affine_map(rank, axes))
        .collect::<Vec<_>>()
        .join(", ");

    parse_attribute(context, &format!("[{maps}]"))
}

/// The affine map for one operand: the result's dimensions on the left, and on
/// the right the operand's own index per axis — a dimension where it walks that
/// axis, `0` where it is stretched, and nothing at all when it is a scalar.
fn affine_map(rank: usize, axes: &OperandAxes) -> String {
    let dimensions = (0..rank)
        .map(|axis| format!("d{axis}"))
        .collect::<Vec<_>>()
        .join(", ");
    let results = match axes {
        None => String::new(),
        Some(axes) => axes
            .iter()
            .map(|walked| match walked {
                Some(axis) => format!("d{axis}"),
                None => STRETCHED_INDEX.to_string(),
            })
            .collect::<Vec<_>>()
            .join(", "),
    };

    format!("affine_map<({dimensions}) -> ({results})>")
}

/// One iterator per axis of the index space: the leading `rank - reductions` are
/// `parallel`, the trailing `reductions` accumulate.
///
/// An element-wise operation has no reduction — every one of its axes is independent.
/// A matrix product has exactly one, the contracted axis, and it comes last because
/// that is the order the affine maps above number the dimensions in.
fn iterator_types<'c>(
    context: &'c Context,
    rank: usize,
    reductions: usize,
) -> Result<Attribute<'c>, MlirError> {
    let iterators = (0..rank)
        .map(|axis| match axis < rank - reductions {
            true => "#linalg.iterator_type<parallel>",
            false => "#linalg.iterator_type<reduction>",
        })
        .collect::<Vec<_>>()
        .join(", ");

    parse_attribute(context, &format!("[{iterators}]"))
}

fn parse_attribute<'c>(context: &'c Context, source: &str) -> Result<Attribute<'c>, MlirError> {
    Attribute::parse(context, source).ok_or_else(|| MlirError::AttributeSyntax(source.to_string()))
}

#[cfg(test)]
mod tests {
    use crate::lower::lower_program;

    use neuro_hir::{
        static_shape, AxisNames, HirExpr, HirExprKind, HirFunction, HirItem, HirParam, HirProgram,
        HirStmt, HirType,
    };
    use shared_types::Span;

    use ast_types::BinaryOp;

    fn span() -> Span {
        Span::new(0, 1)
    }

    fn tensor(element: HirType, extents: &[usize]) -> HirType {
        HirType::Tensor {
            element: Box::new(element),
            shape: static_shape(extents),
            names: AxisNames::default(),
        }
    }

    fn variable(name: &str, ty: HirType) -> HirExpr {
        HirExpr::new(HirExprKind::Variable(name.to_string()), ty, span())
    }

    fn binary(op: BinaryOp, left: HirExpr, right: HirExpr, ty: HirType) -> HirExpr {
        HirExpr::new(
            HirExprKind::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            },
            ty,
            span(),
        )
    }

    /// `func f(a: T, b: T) -> T { <body> }` over one tensor type.
    fn program_over(ty: HirType, body: Vec<HirStmt>) -> HirProgram {
        HirProgram {
            items: vec![HirItem::Function(HirFunction {
                name: "f".to_string(),
                params: vec![
                    HirParam {
                        name: "a".to_string(),
                        ty: ty.clone(),
                        span: span(),
                    },
                    HirParam {
                        name: "b".to_string(),
                        ty: ty.clone(),
                        span: span(),
                    },
                ],
                return_type: ty,
                body,
                span: span(),
            })],
        }
    }

    /// `return a OP b` for a tensor of `element` shaped `extents`.
    fn returns_binary(op: BinaryOp, element: HirType, extents: &[usize]) -> HirProgram {
        let ty = tensor(element, extents);
        let sum = binary(
            op,
            variable("a", ty.clone()),
            variable("b", ty.clone()),
            ty.clone(),
        );

        program_over(
            ty,
            vec![HirStmt::Return {
                value: Some(sum),
                span: span(),
            }],
        )
    }

    /// `func f(a: A, b: B) -> R { return a + b }`, the shape every broadcast case
    /// takes: two operand types that differ from each other and from the result.
    fn returns_sum_over(left: HirType, right: HirType, result: HirType) -> HirProgram {
        let sum = binary(
            BinaryOp::Add,
            variable("a", left.clone()),
            variable("b", right.clone()),
            result.clone(),
        );

        HirProgram {
            items: vec![HirItem::Function(HirFunction {
                name: "f".to_string(),
                params: vec![
                    HirParam {
                        name: "a".to_string(),
                        ty: left,
                        span: span(),
                    },
                    HirParam {
                        name: "b".to_string(),
                        ty: right,
                        span: span(),
                    },
                ],
                return_type: result,
                body: vec![HirStmt::Return {
                    value: Some(sum),
                    span: span(),
                }],
                span: span(),
            })],
        }
    }

    /// A tensor with one `?` axis and one static one, the shape a batch of rows
    /// arriving from outside the program takes.
    fn dynamic_rows(columns: usize) -> HirType {
        HirType::Tensor {
            element: Box::new(HirType::F32),
            shape: vec![None, Some(columns)],
            names: AxisNames::default(),
        }
    }

    #[test]
    fn lowers_tensor_addition_to_a_linalg_generic() {
        let ir = returns_binary(BinaryOp::Add, HirType::F32, &[2, 3]);
        let ir = lower_program(&ir).expect("tensor addition should lower");

        assert!(
            ir.contains("linalg.generic"),
            "expected a linalg.generic op:\n{ir}"
        );
        assert!(
            ir.contains("tensor.empty"),
            "expected the destination tensor:\n{ir}"
        );
        assert!(
            ir.contains("arith.addf"),
            "expected the element-wise body op:\n{ir}"
        );
        assert!(
            ir.contains("linalg.yield"),
            "expected the body terminator:\n{ir}"
        );
        assert!(
            ir.contains("tensor<2x3xf32>"),
            "expected the mapped tensor type:\n{ir}"
        );
    }

    #[test]
    fn lowers_every_arithmetic_operator() {
        for (op, expected) in [
            (BinaryOp::Add, "arith.addf"),
            (BinaryOp::Subtract, "arith.subf"),
            (BinaryOp::Multiply, "arith.mulf"),
            (BinaryOp::Divide, "arith.divf"),
        ] {
            let ir = lower_program(&returns_binary(op, HirType::F64, &[4]))
                .expect("each operator should lower");
            assert!(ir.contains(expected), "expected {expected}:\n{ir}");
        }
    }

    #[test]
    fn integer_division_follows_the_element_signedness() {
        let signed = lower_program(&returns_binary(BinaryOp::Divide, HirType::I32, &[8]))
            .expect("a signed tensor should lower");
        assert!(signed.contains("arith.divsi"), "expected divsi:\n{signed}");

        let unsigned = lower_program(&returns_binary(BinaryOp::Divide, HirType::U32, &[8]))
            .expect("an unsigned tensor should lower");
        assert!(
            unsigned.contains("arith.divui"),
            "expected divui:\n{unsigned}"
        );
    }

    #[test]
    fn lowers_a_chain_through_a_local_binding() {
        let ty = tensor(HirType::F32, &[2, 2]);
        let sum = binary(
            BinaryOp::Add,
            variable("a", ty.clone()),
            variable("b", ty.clone()),
            ty.clone(),
        );
        let scaled = binary(
            BinaryOp::Multiply,
            variable("sum", ty.clone()),
            variable("b", ty.clone()),
            ty.clone(),
        );

        let program = program_over(
            ty.clone(),
            vec![
                HirStmt::VarDecl {
                    name: "sum".to_string(),
                    ty,
                    init: Some(sum),
                    mutable: false,
                    span: span(),
                },
                HirStmt::Return {
                    value: Some(scaled),
                    span: span(),
                },
            ],
        );

        let ir = lower_program(&program).expect("a two-step body should lower");
        assert_eq!(
            ir.matches("linalg.generic").count(),
            2,
            "expected one generic per operator:\n{ir}"
        );
    }

    #[test]
    fn rank_zero_tensors_lower_without_an_iterator() {
        let ir = lower_program(&returns_binary(BinaryOp::Add, HirType::F32, &[]))
            .expect("a rank-0 tensor should lower");

        assert!(ir.contains("tensor<f32>"), "expected a rank-0 type:\n{ir}");
        assert!(
            ir.contains("iterator_types = []"),
            "expected no iterators:\n{ir}"
        );
    }

    #[test]
    fn a_size_one_axis_is_stretched_across_the_result() {
        let result = tensor(HirType::F32, &[2, 3]);
        let row = tensor(HirType::F32, &[1, 3]);

        let ir = lower_program(&returns_sum_over(result.clone(), row, result))
            .expect("a size-1 axis should broadcast");
        assert!(
            ir.contains("affine_map<(d0, d1) -> (0, d1)>"),
            "expected the stretched axis to read index 0:\n{ir}"
        );
        assert!(
            ir.contains("affine_map<(d0, d1) -> (d0, d1)>"),
            "expected the other operand to keep its identity map:\n{ir}"
        );
    }

    #[test]
    fn a_lower_rank_operand_aligns_at_the_trailing_axis() {
        let result = tensor(HirType::F32, &[2, 3]);
        let row = tensor(HirType::F32, &[3]);

        let ir = lower_program(&returns_sum_over(result.clone(), row, result))
            .expect("a lower-rank operand should broadcast");
        assert!(
            ir.contains("affine_map<(d0, d1) -> (d1)>"),
            "expected the operand to supply the innermost axis only:\n{ir}"
        );
    }

    #[test]
    fn a_scalar_operand_broadcasts_with_an_empty_map() {
        let result = tensor(HirType::F32, &[2, 3]);

        let ir = lower_program(&returns_sum_over(result.clone(), HirType::F32, result))
            .expect("a scalar operand should broadcast");
        assert!(
            ir.contains("affine_map<(d0, d1) -> ()>"),
            "expected the scalar to be read at every point:\n{ir}"
        );
        assert!(
            ir.contains("arith.addf"),
            "expected the element-wise body op:\n{ir}"
        );
    }

    #[test]
    fn a_dynamic_extent_sizes_its_destination_from_an_operand() {
        let ty = dynamic_rows(4);

        let ir = lower_program(&returns_sum_over(ty.clone(), ty.clone(), ty))
            .expect("a `?` axis should lower");
        assert!(
            ir.contains("tensor.dim"),
            "expected the extent read off an operand:\n{ir}"
        );
        assert!(
            ir.contains("tensor.empty("),
            "expected the destination to take a size operand:\n{ir}"
        );
        assert!(
            ir.contains("linalg.generic"),
            "expected the body to lower:\n{ir}"
        );
    }

    #[test]
    fn an_extent_unprovable_against_a_dynamic_axis_stays_a_declaration() {
        // A `?` operand extent may or may not be 1 at run time. Stretching it on
        // the chance that it is would silently read the wrong element.
        let program = returns_sum_over(
            dynamic_rows(4),
            tensor(HirType::F32, &[4]),
            tensor(HirType::F32, &[2, 4]),
        );

        let ir = lower_program(&program).expect("the function should still declare");
        assert!(
            !ir.contains("linalg.generic"),
            "expected no body for an unprovable extent:\n{ir}"
        );
        assert!(
            ir.contains("private"),
            "expected the external declaration:\n{ir}"
        );
    }

    #[test]
    fn a_stretched_extent_other_than_one_stays_a_declaration() {
        let program = returns_sum_over(
            tensor(HirType::F32, &[2, 3]),
            tensor(HirType::F32, &[2]),
            tensor(HirType::F32, &[2, 3]),
        );

        let ir = lower_program(&program).expect("the function should still declare");
        assert!(
            !ir.contains("linalg.generic"),
            "expected no body for an incompatible extent:\n{ir}"
        );
    }

    #[test]
    fn an_operand_outranking_the_result_stays_a_declaration() {
        let program = returns_sum_over(
            tensor(HirType::F32, &[3]),
            tensor(HirType::F32, &[2, 3]),
            tensor(HirType::F32, &[3]),
        );

        let ir = lower_program(&program).expect("the function should still declare");
        assert!(
            !ir.contains("linalg.generic"),
            "expected no body for an over-ranked operand:\n{ir}"
        );
    }

    #[test]
    fn a_differing_element_type_stays_a_declaration() {
        // There is no implicit conversion in the language, so a mixed-element
        // operation is a frontend error rather than something to lower.
        let program = returns_sum_over(
            tensor(HirType::F32, &[4]),
            tensor(HirType::F64, &[4]),
            tensor(HirType::F32, &[4]),
        );

        let ir = lower_program(&program).expect("the function should still declare");
        assert!(
            !ir.contains("linalg.generic"),
            "expected no body for mixed element types:\n{ir}"
        );
    }

    /// `func f(a: Tensor<T, [M, K]>, b: Tensor<T, [K, N]>) -> Tensor<T, [M, N]> { return a @ b }`
    fn returns_product(element: HirType, m: usize, k: usize, n: usize) -> HirProgram {
        let left = tensor(element.clone(), &[m, k]);
        let right = tensor(element.clone(), &[k, n]);
        let result = tensor(element, &[m, n]);
        let product = binary(
            BinaryOp::MatMul,
            variable("a", left.clone()),
            variable("b", right.clone()),
            result.clone(),
        );

        HirProgram {
            items: vec![HirItem::Function(HirFunction {
                name: "f".to_string(),
                params: vec![
                    HirParam {
                        name: "a".to_string(),
                        ty: left,
                        span: span(),
                    },
                    HirParam {
                        name: "b".to_string(),
                        ty: right,
                        span: span(),
                    },
                ],
                return_type: result,
                body: vec![HirStmt::Return {
                    value: Some(product),
                    span: span(),
                }],
                span: span(),
            })],
        }
    }

    #[test]
    fn lowers_a_matrix_product_to_a_contracting_linalg_generic() {
        let ir = lower_program(&returns_product(HirType::F32, 2, 3, 4))
            .expect("a matrix product should lower");

        assert!(
            ir.contains("affine_map<(d0, d1, d2) -> (d0, d2)>"),
            "expected the left operand to read a row:\n{ir}"
        );
        assert!(
            ir.contains("affine_map<(d0, d1, d2) -> (d2, d1)>"),
            "expected the right operand to read a column:\n{ir}"
        );
        assert!(
            ir.contains("affine_map<(d0, d1, d2) -> (d0, d1)>"),
            "expected the accumulator to walk the result:\n{ir}"
        );
        assert!(
            ir.contains(r#""parallel", "parallel", "reduction""#),
            "expected the contracted axis to reduce, and only it:\n{ir}"
        );
        assert!(
            ir.contains("arith.mulf") && ir.contains("arith.addf"),
            "expected a multiply-accumulate body:\n{ir}"
        );
        assert!(
            ir.contains("tensor<2x4xf32>"),
            "expected the contracted result type:\n{ir}"
        );
    }

    /// A reduction READS its destination at every point, so an uninitialized
    /// `tensor.empty` would start each sum from whatever was already there.
    #[test]
    fn a_matrix_product_zeroes_its_destination_first() {
        let ir = lower_program(&returns_product(HirType::F32, 2, 2, 2))
            .expect("a matrix product should lower");

        let fill = ir
            .find("arith.constant")
            .expect("expected the element's zero");
        let product = ir.find(r#""reduction""#).expect("expected the contraction");
        assert!(fill < product, "the fill must precede the product:\n{ir}");
        assert!(
            ir.contains("0.000000e+00"),
            "expected a zero of the element type:\n{ir}"
        );
    }

    #[test]
    fn an_integer_matrix_product_accumulates_with_addi() {
        let ir = lower_program(&returns_product(HirType::I32, 2, 2, 2))
            .expect("a matrix product should lower");

        assert!(
            ir.contains("arith.muli") && ir.contains("arith.addi"),
            "expected the integer multiply-accumulate:\n{ir}"
        );
    }

    /// `@` contracts an axis it must know the length of, and a `?` supplies none.
    #[test]
    fn a_dynamic_extent_leaves_a_matrix_product_a_declaration() {
        let left = dynamic_rows(3);
        let right = tensor(HirType::F32, &[3, 4]);
        let result = HirType::Tensor {
            element: Box::new(HirType::F32),
            shape: vec![None, Some(4)],
            names: AxisNames(vec![None, None]),
        };
        let product = binary(
            BinaryOp::MatMul,
            variable("a", left.clone()),
            variable("b", right.clone()),
            result.clone(),
        );
        let program = HirProgram {
            items: vec![HirItem::Function(HirFunction {
                name: "f".to_string(),
                params: vec![
                    HirParam {
                        name: "a".to_string(),
                        ty: left,
                        span: span(),
                    },
                    HirParam {
                        name: "b".to_string(),
                        ty: right,
                        span: span(),
                    },
                ],
                return_type: result,
                body: vec![HirStmt::Return {
                    value: Some(product),
                    span: span(),
                }],
                span: span(),
            })],
        };
        let ir = lower_program(&program).expect("the program should still lower");

        assert!(
            !ir.contains(r#""reduction""#),
            "a dynamic extent has no contraction to emit:\n{ir}"
        );
    }

    #[test]
    fn a_scalar_body_stays_a_declaration() {
        // Scalar arithmetic is permanently the inkwell backend's, per 2C's decision.
        let program = HirProgram {
            items: vec![HirItem::Function(HirFunction {
                name: "add".to_string(),
                params: vec![
                    HirParam {
                        name: "a".to_string(),
                        ty: HirType::I32,
                        span: span(),
                    },
                    HirParam {
                        name: "b".to_string(),
                        ty: HirType::I32,
                        span: span(),
                    },
                ],
                return_type: HirType::I32,
                body: vec![HirStmt::Return {
                    value: Some(binary(
                        BinaryOp::Add,
                        variable("a", HirType::I32),
                        variable("b", HirType::I32),
                        HirType::I32,
                    )),
                    span: span(),
                }],
                span: span(),
            })],
        };

        let ir = lower_program(&program).expect("a scalar function should still declare");
        assert!(
            !ir.contains("linalg.generic"),
            "expected scalar arithmetic to stay on inkwell:\n{ir}"
        );
    }
}
