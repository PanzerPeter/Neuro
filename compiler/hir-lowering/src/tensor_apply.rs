//! The functional traversals `.map(f)`, `.zip(other, f)`, and `.reduce(init, f)`.
//!
//! Reached from the method-call arm of `lower_expr`. Every argument is an ordinary value
//! here, unlike a reduction's `axis:`, so each is lowered the usual way.
//!
//! The type checker has already validated the call, so anything that does not fit below is
//! a divergence between the two surfaces and becomes a `LoweringError`, as this slice's
//! CONTEXT.md requires.

use ast_types::Expr;
use neuro_hir::{HirExpr, HirExprKind, HirTensorApply, HirType};
use shared_types::Span;

use crate::{Lowerer, LoweringError};

pub(crate) const MAP_METHOD: &str = "map";
pub(crate) const ZIP_METHOD: &str = "zip";
pub(crate) const REDUCE_METHOD: &str = "reduce";

/// Whether `method` names one of the three functional traversals.
pub(crate) fn is_apply_method(method: &str) -> bool {
    matches!(method, MAP_METHOD | ZIP_METHOD | REDUCE_METHOD)
}

fn malformed(detail: String) -> LoweringError {
    LoweringError::Malformed { detail }
}

impl Lowerer {
    /// Lower one traversal into a [`HirExprKind::TensorApply`].
    ///
    /// The receiver is left as it is: a traversal reads the buffer it walks, so nothing is
    /// moved and the receiver keeps its binding.
    pub(crate) fn lower_tensor_apply(
        &mut self,
        receiver: HirExpr,
        method: &str,
        args: &[Expr],
        span: Span,
    ) -> Result<HirExpr, LoweringError> {
        let HirType::Tensor { shape, names, .. } = receiver.ty.referent().clone() else {
            return Err(malformed(format!(
                "`.{method}` reached lowering on a non-tensor receiver"
            )));
        };
        let kind = apply_kind(method)?;
        // The function is the last argument of all three, and the one before it is the
        // second tensor of a `.zip` or the seed of a `.reduce`.
        let Some((callee, leading)) = args.split_last() else {
            return Err(malformed(format!(
                "`.{method}` reached lowering with no function"
            )));
        };
        // The function is lowered first so a `.reduce`'s seed can take its type from the
        // accumulator parameter, the way the checker resolved it: an untyped `0.0` folded
        // over an `f32` tensor is an `f32`, and lowering it without the hint would build
        // an accumulator the function cannot be called with.
        let callee = self.lower_expr(callee, None)?;
        let HirType::Function { params, ret } = &callee.ty else {
            return Err(malformed(format!(
                "`.{method}` reached lowering with a non-function argument"
            )));
        };
        let seed_ty = match kind {
            HirTensorApply::Reduce => params.first().cloned(),
            _ => None,
        };
        let ret = (**ret).clone();
        let operand = match leading.first() {
            Some(entry) => Some(Box::new(self.lower_expr(entry, seed_ty.as_ref())?)),
            None => None,
        };

        // A fold answers one value for the whole buffer, so it carries the seed's type.
        // The other two rebuild the receiver's shape over whatever the function returns,
        // axis names included: neither changes which axis is which.
        let ty = match kind {
            HirTensorApply::Reduce => match &operand {
                Some(seed) => seed.ty.clone(),
                None => {
                    return Err(malformed(
                        "`.reduce` reached lowering with no seed".to_string(),
                    ))
                }
            },
            _ => HirType::Tensor {
                element: Box::new(ret),
                shape,
                names,
            },
        };

        Ok(HirExpr::new(
            HirExprKind::TensorApply {
                kind,
                receiver: Box::new(receiver),
                operand,
                callee: Box::new(callee),
            },
            ty,
            span,
        ))
    }
}

fn apply_kind(method: &str) -> Result<HirTensorApply, LoweringError> {
    match method {
        MAP_METHOD => Ok(HirTensorApply::Map),
        ZIP_METHOD => Ok(HirTensorApply::Zip),
        REDUCE_METHOD => Ok(HirTensorApply::Reduce),
        other => Err(malformed(format!(
            "`.{other}` is not a functional traversal"
        ))),
    }
}
