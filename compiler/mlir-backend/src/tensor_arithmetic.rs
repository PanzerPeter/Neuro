use crate::{errors::MlirError, lower::map_type};

use ast_types::BinaryOp;
use melior::{
    dialect::{arith, func},
    ir::{
        attribute::DenseI32ArrayAttribute, operation::OperationBuilder, Attribute, Block,
        BlockLike, Identifier, Location, Operation, Region, RegionLike, Type, Value,
    },
    Context,
};
use neuro_hir::{HirExpr, HirExprKind, HirFunction, HirStmt, HirType};

/// `linalg.generic`'s operand split for an element-wise binary operation: two
/// inputs, then the destination the result is written into.
const ELEMENTWISE_OPERANDS: [i32; 2] = [2, 1];

/// How many region arguments `linalg.generic` passes an element-wise binary body:
/// one per operand, the destination's included.
const ELEMENTWISE_BODY_ARGUMENTS: usize = 3;

/// The `arith` operation that carries one element of an element-wise tensor
/// operation. A function pointer rather than an enum because every candidate
/// already has this exact shape in `melior`.
type ScalarOp = for<'c, 'a> fn(Value<'c, 'a>, Value<'c, 'a>, Location<'c>) -> Operation<'c>;

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
    // Cheap filter first: a function that does not hand a static tensor back
    // cannot be one of these, and every scalar function in the program hits it.
    if static_tensor(&function.return_type).is_none() {
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

/// The element type and rank of a tensor whose every extent is known at compile time.
///
/// `None` for every other type, a dynamic `?` axis included: the destination this
/// path builds is a `tensor.empty`, which needs one size operand per dynamic
/// dimension, and nothing here computes those.
fn static_tensor(ty: &HirType) -> Option<(&HirType, usize)> {
    let HirType::Tensor { element, shape, .. } = ty else {
        return None;
    };

    shape
        .iter()
        .all(Option::is_some)
        .then(|| (element.as_ref(), shape.len()))
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
    let Some((element, rank)) = static_tensor(&expression.ty) else {
        return Ok(None);
    };
    // Broadcasting is the next 2C item. Requiring both operands to be the result's
    // own type is what makes a single identity indexing map correct for all three.
    if left.ty != expression.ty || right.ty != expression.ty {
        return Ok(None);
    }
    let Some(scalar) = scalar_op(*op, element) else {
        return Ok(None);
    };
    let Some(lhs) = build_expression(context, location, block, left, scope)? else {
        return Ok(None);
    };
    let Some(rhs) = build_expression(context, location, block, right, scope)? else {
        return Ok(None);
    };

    let tensor_type = map_type(context, &expression.ty)?;
    let element_type = map_type(context, element)?;

    let destination = block
        .append_operation(empty_tensor(location, tensor_type)?)
        .result(0)?
        .into();

    let generic = generic_op(
        context,
        location,
        ElementwiseOperands {
            inputs: [lhs, rhs],
            destination,
        },
        tensor_type,
        element_type,
        rank,
        scalar,
    )?;

    Ok(Some(block.append_operation(generic).result(0)?.into()))
}

/// The values one `linalg.generic` consumes, grouped the way its operand segments
/// are: the inputs it reads, then the destination it writes into.
struct ElementwiseOperands<'c, 'a> {
    inputs: [Value<'c, 'a>; 2],
    destination: Value<'c, 'a>,
}

/// Assemble the `linalg.generic` itself: identity maps over every operand, one
/// `parallel` iterator per axis, and the scalar body as its region.
fn generic_op<'c>(
    context: &'c Context,
    location: Location<'c>,
    operands: ElementwiseOperands<'c, '_>,
    tensor_type: Type<'c>,
    element_type: Type<'c>,
    rank: usize,
    scalar: ScalarOp,
) -> Result<Operation<'c>, MlirError> {
    let body = Region::new();
    body.append_block(scalar_body(location, element_type, scalar)?);

    let [lhs, rhs] = operands.inputs;

    Ok(OperationBuilder::new("linalg.generic", location)
        .add_operands(&[lhs, rhs, operands.destination])
        .add_results(&[tensor_type])
        .add_attributes(&[
            (
                Identifier::new(context, "indexing_maps"),
                identity_maps(context, rank)?,
            ),
            (
                Identifier::new(context, "iterator_types"),
                parallel_iterators(context, rank)?,
            ),
            (
                Identifier::new(context, "operandSegmentSizes"),
                DenseI32ArrayAttribute::new(context, &ELEMENTWISE_OPERANDS).into(),
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

/// The destination `linalg.generic` writes its result into.
fn empty_tensor<'c>(location: Location<'c>, tensor: Type<'c>) -> Result<Operation<'c>, MlirError> {
    Ok(OperationBuilder::new("tensor.empty", location)
        .add_results(&[tensor])
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

/// One identity affine map per `linalg.generic` operand: every operand is walked
/// in the same order, which is what makes the operation element-wise.
fn identity_maps<'c>(context: &'c Context, rank: usize) -> Result<Attribute<'c>, MlirError> {
    let dimensions = (0..rank)
        .map(|axis| format!("d{axis}"))
        .collect::<Vec<_>>()
        .join(", ");
    let map = format!("affine_map<({dimensions}) -> ({dimensions})>");

    parse_attribute(
        context,
        &format!("[{}]", vec![map; ELEMENTWISE_BODY_ARGUMENTS].join(", ")),
    )
}

/// Every axis of an element-wise operation is independent, so every iterator is
/// `parallel`; a reduction is 2B's work and stays on inkwell.
fn parallel_iterators<'c>(context: &'c Context, rank: usize) -> Result<Attribute<'c>, MlirError> {
    let iterators = vec!["#linalg.iterator_type<parallel>"; rank].join(", ");

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
    fn a_shape_mismatch_stays_a_declaration() {
        // Broadcasting is the next 2C item, so unequal operand shapes are not
        // lowered here rather than being lowered wrongly.
        let result = tensor(HirType::F32, &[2, 3]);
        let other = tensor(HirType::F32, &[3]);
        let sum = binary(
            BinaryOp::Add,
            variable("a", result.clone()),
            variable("b", other.clone()),
            result.clone(),
        );

        let program = HirProgram {
            items: vec![HirItem::Function(HirFunction {
                name: "f".to_string(),
                params: vec![
                    HirParam {
                        name: "a".to_string(),
                        ty: result.clone(),
                        span: span(),
                    },
                    HirParam {
                        name: "b".to_string(),
                        ty: other,
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
        };

        let ir = lower_program(&program).expect("the function should still declare");
        assert!(
            !ir.contains("linalg.generic"),
            "expected no body for a broadcast:\n{ir}"
        );
        assert!(
            ir.contains("private"),
            "expected the external declaration:\n{ir}"
        );
    }

    #[test]
    fn a_dynamic_extent_stays_a_declaration() {
        let ty = HirType::Tensor {
            element: Box::new(HirType::F32),
            shape: vec![None, Some(4)],
            names: AxisNames::default(),
        };
        let sum = binary(
            BinaryOp::Add,
            variable("a", ty.clone()),
            variable("b", ty.clone()),
            ty.clone(),
        );
        let program = program_over(
            ty,
            vec![HirStmt::Return {
                value: Some(sum),
                span: span(),
            }],
        );

        let ir = lower_program(&program).expect("a `?` axis should still declare");
        assert!(
            ir.contains("tensor<?x4xf32>"),
            "expected the dynamic type in the signature:\n{ir}"
        );
        assert!(
            !ir.contains("linalg.generic"),
            "expected no body for a `?` axis:\n{ir}"
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
