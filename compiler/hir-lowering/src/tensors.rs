//! Tensor value construction: coerced nested array literals and the six
//! `Tensor::<T, [...]>::ctor(...)` construction helpers.
//!
//! Reached from the array-literal and call arms of `lower_expr_uncoerced`. The type
//! checker has already validated shape, rank, element type, and arity, so the work
//! here is flattening the literal into row-major buffer order and picking the node.

use ast_types::{Expr, GenericArg};
use neuro_hir::{AxisNames, HirExpr, HirExprKind, HirType};
use shared_types::Literal;

use crate::{Lowerer, LoweringError};

/// The prelude name a tensor constructor is qualified by. A program declaring its own
/// `Tensor` keeps it, so the call arm checks the struct and enum tables first.
pub(crate) const TENSOR_TYPE_NAME: &str = "Tensor";

const CTOR_ZEROS: &str = "zeros";
const CTOR_ONES: &str = "ones";
const CTOR_IDENTITY: &str = "identity";
const CTOR_RANDOM_NORMAL: &str = "random_normal";
const CTOR_SCALAR: &str = "scalar";
const CTOR_FROM: &str = "from";

/// Whether `ctor` names one of the construction helpers.
pub(crate) fn is_tensor_constructor(ctor: &str) -> bool {
    matches!(
        ctor,
        CTOR_ZEROS | CTOR_ONES | CTOR_IDENTITY | CTOR_RANDOM_NORMAL | CTOR_SCALAR | CTOR_FROM
    )
}

/// The `dims:` argument of a constructor whose fixed arguments come first: the array literal
/// at `at`, when the call passes one.
fn dims_argument(args: &[Expr], at: usize) -> Option<&[Expr]> {
    match args.get(at) {
        Some(Expr::ArrayLiteral { elements, .. }) => Some(elements),
        _ => None,
    }
}

/// `node` after the statements that bind the names its constructor introduced, in a block
/// whose value is the node.
fn with_bindings(mut stmts: Vec<neuro_hir::HirStmt>, node: HirExpr) -> HirExpr {
    if stmts.is_empty() {
        return node;
    }
    let (ty, span) = (node.ty.clone(), node.span);
    stmts.push(neuro_hir::HirStmt::Expr(node));
    HirExpr::new(HirExprKind::Block { stmts }, ty, span)
}

/// The number of elements a tensor of `shape` holds. The rank-0 shape is one element,
/// which is what makes the empty product the right answer.
pub(crate) fn tensor_element_count(shape: &[usize]) -> usize {
    shape.iter().product()
}

impl Lowerer {
    /// Lower a nested array literal that a `Tensor<T, [...]>` annotation coerced.
    pub(crate) fn lower_tensor_literal(
        &mut self,
        elements: &[Expr],
        element: &HirType,
        shape: &[usize],
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        let mut flat = Vec::with_capacity(tensor_element_count(shape));
        self.flatten_tensor_literal(elements, element, shape, &mut flat)?;
        Ok(HirExpr::new(
            HirExprKind::TensorLiteral { elements: flat },
            HirType::Tensor {
                element: Box::new(element.clone()),
                shape: neuro_hir::static_shape(shape),
                names: AxisNames::default(),
            },
            span,
        ))
    }

    /// Append the leaves of one nesting level to `out` in row-major order.
    fn flatten_tensor_literal(
        &mut self,
        elements: &[Expr],
        element: &HirType,
        shape: &[usize],
        out: &mut Vec<HirExpr>,
    ) -> Result<(), LoweringError> {
        let Some((_, rest)) = shape.split_first() else {
            return Err(LoweringError::Malformed {
                detail: "a rank-0 tensor has no array literal form".to_string(),
            });
        };
        for expr in elements {
            if rest.is_empty() {
                out.push(self.lower_expr(expr, Some(element))?);
                continue;
            }
            let Expr::ArrayLiteral {
                elements: nested, ..
            } = expr
            else {
                return Err(LoweringError::Malformed {
                    detail: "a tensor literal axis holds nested literals".to_string(),
                });
            };
            self.flatten_tensor_literal(nested, element, rest, out)?;
        }
        Ok(())
    }

    /// Lower `Tensor::<T, [...]>::ctor(args)` and its annotation-driven spelling.
    ///
    /// The tensor type comes from the turbofish when one was written and from the
    /// surrounding expectation otherwise, the same two sources the type checker used
    /// to accept the call.
    pub(crate) fn lower_tensor_construction(
        &mut self,
        ctor: &str,
        type_args: &[GenericArg],
        args: &[Expr],
        expected: Option<&HirType>,
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        let ty = match type_args {
            [GenericArg::Type(annotation)] => self.resolve_type(annotation)?,
            _ => match expected {
                Some(ty @ HirType::Tensor { .. }) => ty.clone(),
                _ => {
                    return Err(LoweringError::Malformed {
                        detail: format!("`Tensor::{ctor}` has no tensor type to build"),
                    });
                }
            },
        };
        let HirType::Tensor { element, shape, .. } = &ty else {
            return Err(LoweringError::Malformed {
                detail: format!("`Tensor::{ctor}` did not resolve to a tensor type"),
            });
        };
        let element = (**element).clone();
        let open_shape = shape.clone();
        let annotation = match type_args {
            [GenericArg::Type(annotation)] => Some(annotation),
            _ => None,
        };

        // The open axes of a constructor take their extents from `dims:`, the last argument
        // after the ones the constructor fixes. `from` and `scalar` take static shapes only.
        match ctor {
            CTOR_ZEROS | CTOR_ONES | CTOR_IDENTITY => {
                let value = match ctor {
                    CTOR_ONES => 1.0,
                    _ => 0.0,
                };
                let (stmts, dims) =
                    self.constructor_dims(annotation, &open_shape, dims_argument(args, 0), span)?;
                let node = match ctor {
                    CTOR_IDENTITY => HirExprKind::TensorIdentity { dims },
                    _ => HirExprKind::TensorFill {
                        value: Box::new(self.tensor_fill_constant(&element, value, span)?),
                        dims,
                    },
                };
                return Ok(with_bindings(stmts, HirExpr::new(node, ty, span)));
            }
            CTOR_RANDOM_NORMAL => {
                let [mean, std, ..] = args else {
                    return Err(LoweringError::Malformed {
                        detail: "`Tensor::random_normal` takes a mean and a standard deviation"
                            .to_string(),
                    });
                };
                let mean = self.lower_expr(mean, Some(&element))?;
                let std = self.lower_expr(std, Some(&element))?;
                let (stmts, dims) =
                    self.constructor_dims(annotation, &open_shape, dims_argument(args, 2), span)?;
                let node = HirExpr::new(
                    HirExprKind::TensorRandomNormal {
                        mean: Box::new(mean),
                        std: Box::new(std),
                        dims,
                    },
                    ty,
                    span,
                );
                return Ok(with_bindings(stmts, node));
            }
            _ => {}
        }
        let shape = crate::static_extents(shape)?;
        let kind = match ctor {
            CTOR_SCALAR => {
                let [value] = args else {
                    return Err(LoweringError::Malformed {
                        detail: "`Tensor::scalar` takes one value".to_string(),
                    });
                };
                HirExprKind::TensorLiteral {
                    elements: vec![self.lower_expr(value, Some(&element))?],
                }
            }
            CTOR_FROM => {
                let [Expr::ArrayLiteral { elements, .. }] = args else {
                    return Err(LoweringError::Malformed {
                        detail: "`Tensor::from` takes a nested array literal".to_string(),
                    });
                };
                let mut flat = Vec::with_capacity(tensor_element_count(&shape));
                self.flatten_tensor_literal(elements, &element, &shape, &mut flat)?;
                HirExprKind::TensorLiteral { elements: flat }
            }
            other => {
                return Err(LoweringError::Malformed {
                    detail: format!("`Tensor` has no constructor named '{other}'"),
                });
            }
        };
        Ok(HirExpr::new(kind, ty, span))
    }

    /// The `0` / `1` a `zeros()` / `ones()` fill writes, typed as the tensor's element
    /// so the backend emits it at the buffer's width without a conversion.
    fn tensor_fill_constant(
        &self,
        element: &HirType,
        value: f64,
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        let literal = match element {
            HirType::Bool => Literal::Boolean(value != 0.0),
            HirType::F16 | HirType::BF16 | HirType::F32 | HirType::F64 => {
                Literal::Float(value, None)
            }
            _ => Literal::Integer(value as i128, None),
        };
        Ok(HirExpr::new(
            HirExprKind::Literal(literal),
            element.clone(),
            span,
        ))
    }
}
