//! The `?` operator: unwrap an `Option<T>` / `Result<T, E>` or leave the enclosing
//! function carrying the failure variant on.
//!
//! Reached from the `lower_expr_uncoerced` dispatch in this module's `mod.rs`.
//! Every file here adds methods to the same `impl Lowerer` block.

use ast_types::Expr;
use neuro_hir::{
    HirBindingSource, HirExpr, HirExprKind, HirMatchArm, HirMatchBinding, HirMatchTest, HirStmt,
    HirType,
};

use crate::{Lowerer, LoweringError};

/// The fallible enum whose failure variant carries a payload to forward.
const RESULT_ENUM: &str = "Result";
/// The variant `?` propagates out of a `Result`.
const RESULT_FAILURE_VARIANT: &str = "Err";
/// The variant `?` propagates out of an `Option`. It carries no payload, so the
/// propagated value is rebuilt from the variant alone.
const OPTION_FAILURE_VARIANT: &str = "None";

impl Lowerer {
    /// Lower `operand?` into the `match` the spec defines it as:
    ///
    /// ```text
    /// match operand {
    ///     Ok(__try_N)  => __try_N,
    ///     _            => return Err(__try_err_N)
    /// }
    /// ```
    ///
    /// Desugaring here rather than adding a HIR node keeps both backends unaware of `?`:
    /// the failure arm is an ordinary block that terminates with a `return`, which the
    /// existing per-arm basic-block chain already handles.
    ///
    /// The failure value is rebuilt against the ENCLOSING FUNCTION's return instance, not
    /// the operand's — a `Result<u8, E>` propagating out of a `-> Result<i32, E>` function
    /// must produce the latter. The checker has verified the two share an error type.
    pub(super) fn lower_try(
        &mut self,
        operand: &Expr,
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        let scrutinee = self.lower_expr(operand, None)?;
        let (success_tag, payload) = self.success_variant(&scrutinee.ty)?;
        let base = self
            .fallible_base(&scrutinee.ty)
            .ok_or_else(|| Self::not_fallible(&scrutinee.ty))?
            .to_string();

        let index = self.try_counter;
        self.try_counter += 1;

        let unwrapped = HirExpr::new(
            HirExprKind::Variable(format!("__try_{}", index)),
            payload.clone(),
            span,
        );
        let success = HirMatchArm {
            tests: vec![HirMatchTest::Tag { tag: success_tag }],
            bindings: vec![HirMatchBinding {
                name: format!("__try_{}", index),
                ty: payload.clone(),
                source: HirBindingSource::EnumPayload { slot: 0 },
            }],
            guard: None,
            body: unwrapped,
        };

        let failure = self.propagation_arm(&base, &scrutinee.ty, &payload, index, span)?;

        Ok(HirExpr::new(
            HirExprKind::Match {
                scrutinee: Box::new(scrutinee),
                arms: vec![success, failure],
            },
            payload,
            span,
        ))
    }

    /// Build the arm that forwards the failure: it binds the `Err` payload (nothing for
    /// an `Option`) and returns the failure variant of the function's own return instance.
    fn propagation_arm(
        &mut self,
        base: &str,
        scrutinee_ty: &HirType,
        payload: &HirType,
        index: usize,
        span: shared_types::Span,
    ) -> Result<HirMatchArm, LoweringError> {
        let HirType::Enum(return_instance) = self.current_return.clone() else {
            return Err(LoweringError::Malformed {
                detail: format!(
                    "`?` in a function returning {:?}, which cannot carry a failure",
                    self.current_return
                ),
            });
        };

        let (bindings, forwarded) =
            if base == RESULT_ENUM {
                let HirType::Enum(instance) = scrutinee_ty.referent() else {
                    return Err(Self::not_fallible(scrutinee_ty));
                };
                let (_, fields) = self.enum_variant(instance, RESULT_FAILURE_VARIANT)?;
                let err_ty = fields.first().map(|(_, t)| t.clone()).ok_or_else(|| {
                    LoweringError::Malformed {
                        detail: format!(
                            "`{}::{}` carries no payload",
                            instance, RESULT_FAILURE_VARIANT
                        ),
                    }
                })?;
                let name = format!("__try_err_{}", index);
                let (tag, _) = self.enum_variant(&return_instance, RESULT_FAILURE_VARIANT)?;
                let value = HirExpr::new(HirExprKind::Variable(name.clone()), err_ty.clone(), span);
                (
                    vec![HirMatchBinding {
                        name,
                        ty: err_ty,
                        source: HirBindingSource::EnumPayload { slot: 0 },
                    }],
                    self.build_enum_construct(
                        &return_instance,
                        RESULT_FAILURE_VARIANT,
                        tag,
                        vec![value],
                        span,
                    ),
                )
            } else {
                let (tag, _) = self.enum_variant(&return_instance, OPTION_FAILURE_VARIANT)?;
                (
                    Vec::new(),
                    self.build_enum_construct(
                        &return_instance,
                        OPTION_FAILURE_VARIANT,
                        tag,
                        Vec::new(),
                        span,
                    ),
                )
            };

        // The arm never yields: it is typed as the match's payload only so the two arms
        // agree, and the `return` terminates the block before any value is stored.
        let body = HirExpr::new(
            HirExprKind::Block {
                stmts: vec![HirStmt::Return {
                    value: Some(forwarded),
                    span,
                }],
            },
            payload.clone(),
            span,
        );

        Ok(HirMatchArm {
            tests: vec![HirMatchTest::Wildcard],
            bindings,
            guard: None,
            body,
        })
    }
}
