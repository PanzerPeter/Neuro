//! The tensor reductions `.sum()`, `.mean()`, `.max()`, and `.min()`.
//!
//! Reached from the method-call arm of `lower_expr`, before the arguments are lowered:
//! an `axis:` argument may be a dimension NAME, which resolves against the receiver's own
//! shape and would be an unresolved variable if it reached `lower_expr`.
//!
//! The type checker has already validated the call, so an axis that does not resolve here
//! is a divergence between the two surfaces and becomes a `LoweringError`, as this
//! slice's CONTEXT.md requires.

use ast_types::{Expr, UnaryOp};
use neuro_hir::{AxisNames, HirExpr, HirExprKind, HirReduceOp, HirType};
use shared_types::{Literal, Span};

use crate::{Lowerer, LoweringError};

pub(crate) const SUM_METHOD: &str = "sum";
pub(crate) const MEAN_METHOD: &str = "mean";
pub(crate) const MAX_METHOD: &str = "max";
pub(crate) const MIN_METHOD: &str = "min";

/// Whether `method` names one of the four reductions.
pub(crate) fn is_reduce_method(method: &str) -> bool {
    matches!(method, SUM_METHOD | MEAN_METHOD | MAX_METHOD | MIN_METHOD)
}

fn malformed(detail: String) -> LoweringError {
    LoweringError::Malformed { detail }
}

impl Lowerer {
    /// Lower one reduction into a [`HirExprKind::TensorReduce`].
    ///
    /// The receiver is left as it is: a reduction reads the buffer it summarises, so
    /// unlike a shape cast it moves nothing and the receiver keeps its binding.
    pub(crate) fn lower_tensor_reduce(
        &mut self,
        receiver: HirExpr,
        method: &str,
        args: &[Expr],
        span: Span,
    ) -> Result<HirExpr, LoweringError> {
        let HirType::Tensor {
            element,
            shape,
            names,
        } = receiver.ty.referent().clone()
        else {
            return Err(malformed(format!(
                "`.{method}` reached lowering on a non-tensor receiver"
            )));
        };
        let shape = crate::static_extents(&shape)?;
        let op = reduce_op(method)?;

        let Some(entry) = args.first() else {
            return Ok(HirExpr::new(
                HirExprKind::TensorReduce {
                    receiver: Box::new(receiver),
                    op,
                    axis: None,
                },
                (*element).clone(),
                span,
            ));
        };

        let axis = resolve_axis(&shape, &names, entry, method)?;
        let mut result_shape = shape.clone();
        result_shape.remove(axis);
        let mut result_names = names.0.clone();
        result_names.remove(axis);
        Ok(HirExpr::new(
            HirExprKind::TensorReduce {
                receiver: Box::new(receiver),
                op,
                axis: Some(axis),
            },
            HirType::Tensor {
                element,
                shape: neuro_hir::static_shape(&result_shape),
                names: AxisNames(result_names),
            },
            span,
        ))
    }
}

fn reduce_op(method: &str) -> Result<HirReduceOp, LoweringError> {
    match method {
        SUM_METHOD => Ok(HirReduceOp::Sum),
        MEAN_METHOD => Ok(HirReduceOp::Mean),
        MAX_METHOD => Ok(HirReduceOp::Max),
        MIN_METHOD => Ok(HirReduceOp::Min),
        other => Err(malformed(format!("`.{other}` is not a reduction"))),
    }
}

/// The receiver axis an `axis:` argument names: a dimension name, a non-negative index,
/// or a negative index counting from the end.
fn resolve_axis(
    shape: &[usize],
    names: &AxisNames,
    entry: &Expr,
    method: &str,
) -> Result<usize, LoweringError> {
    if let Expr::Identifier(ident) = entry {
        return names.position_of(&ident.name).ok_or_else(|| {
            malformed(format!(
                "`.{method}` reached lowering naming dimension '{}', which its receiver's \
                 type does not declare",
                ident.name
            ))
        });
    }
    let value = const_integer(entry).ok_or_else(|| {
        malformed(format!(
            "`.{method}` reached lowering with a non-constant axis"
        ))
    })?;
    let rank = i128::try_from(shape.len()).map_err(|_| {
        malformed(format!(
            "`.{method}` reached lowering on an unrepresentable rank"
        ))
    })?;
    let resolved = if value < 0 { value + rank } else { value };
    usize::try_from(resolved)
        .ok()
        .filter(|axis| *axis < shape.len())
        .ok_or_else(|| {
            malformed(format!(
                "`.{method}` reached lowering with axis {value} out of range"
            ))
        })
}

/// The value of an integer constant expression written as an axis.
fn const_integer(expr: &Expr) -> Option<i128> {
    match expr {
        Expr::Literal(Literal::Integer(value, _), _) => Some(*value),
        Expr::Paren(inner, _) => const_integer(inner),
        Expr::Unary {
            op: UnaryOp::Negate,
            operand,
            ..
        } => const_integer(operand).map(|value| -value),
        _ => None,
    }
}
