//! Elementwise math: `.exp()`, `.log()`, `.sqrt()`, `.tanh()`, `.abs()`, `.pow(p)`.
//!
//! Reached from the method-call arm of `lower_expr`. A scalar and a tensor receiver lower
//! to the same [`HirExprKind::Math`], typed as the receiver's value: a backend reads the
//! scalar or per-element form off that type, and the derivative transform needs one rule
//! per function rather than two.

use ast_types::Expr;
use neuro_hir::{HirExpr, HirExprKind, HirMathOp, HirType};
use shared_types::Span;

use crate::{Lowerer, LoweringError};

/// The function a math method names, `None` for any other method.
pub(crate) fn math_op(method: &str) -> Option<HirMathOp> {
    match method {
        "exp" => Some(HirMathOp::Exp),
        "log" => Some(HirMathOp::Log),
        "sqrt" => Some(HirMathOp::Sqrt),
        "tanh" => Some(HirMathOp::Tanh),
        "abs" => Some(HirMathOp::Abs),
        "pow" => Some(HirMathOp::Pow),
        _ => None,
    }
}

impl Lowerer {
    /// Lower one math call on `operand`. The checker has already required `.pow`'s single
    /// exponent to be a scalar of the element type and every other method to take none.
    pub(crate) fn lower_elementwise_math(
        &mut self,
        operand: HirExpr,
        op: HirMathOp,
        args: &[Expr],
        span: Span,
    ) -> Result<HirExpr, LoweringError> {
        let ty = operand.ty.referent().clone();
        let element = match &ty {
            HirType::Tensor { element, .. } => (**element).clone(),
            scalar => scalar.clone(),
        };
        let exponent = match (op, args) {
            (HirMathOp::Pow, [exponent]) => {
                Some(Box::new(self.lower_expr(exponent, Some(&element))?))
            }
            (HirMathOp::Pow, _) => {
                return Err(LoweringError::Malformed {
                    detail: "`.pow` reached lowering without exactly one exponent".to_string(),
                })
            }
            _ => None,
        };
        Ok(HirExpr::new(
            HirExprKind::Math {
                op,
                operand: Box::new(operand),
                exponent,
            },
            ty,
            span,
        ))
    }
}
