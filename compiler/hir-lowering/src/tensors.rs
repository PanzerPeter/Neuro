//! Tensor value construction: coerced nested array literals and the six
//! `Tensor::<T, [...]>::ctor(...)` construction helpers.
//!
//! Reached from the array-literal and call arms of `lower_expr_uncoerced`. The type
//! checker has already validated shape, rank, element type, and arity, so the work
//! here is flattening the literal into row-major buffer order and picking the node.

use ast_types::{Expr, GenericArg};
use neuro_hir::{HirExpr, HirExprKind, HirType};
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
                shape: shape.to_vec(),
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
    /// surrounding expectation otherwise — the same two sources the type checker used
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
                    })
                }
            },
        };
        let HirType::Tensor { element, shape } = &ty else {
            return Err(LoweringError::Malformed {
                detail: format!("`Tensor::{ctor}` did not resolve to a tensor type"),
            });
        };
        let element = (**element).clone();
        let shape = shape.clone();

        let kind = match ctor {
            CTOR_ZEROS => HirExprKind::TensorFill {
                value: Box::new(self.tensor_fill_constant(&element, 0.0, span)?),
            },
            CTOR_ONES => HirExprKind::TensorFill {
                value: Box::new(self.tensor_fill_constant(&element, 1.0, span)?),
            },
            CTOR_IDENTITY => HirExprKind::TensorIdentity,
            CTOR_RANDOM_NORMAL => {
                let [mean, std] = args else {
                    return Err(LoweringError::Malformed {
                        detail: "`Tensor::random_normal` takes a mean and a standard deviation"
                            .to_string(),
                    });
                };
                HirExprKind::TensorRandomNormal {
                    mean: Box::new(self.lower_expr(mean, Some(&element))?),
                    std: Box::new(self.lower_expr(std, Some(&element))?),
                }
            }
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
                })
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
            _ => Literal::Integer(value as i64, None),
        };
        Ok(HirExpr::new(
            HirExprKind::Literal(literal),
            element.clone(),
            span,
        ))
    }
}
