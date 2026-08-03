//! The `??` operator: unwrap an `Option<T>` / `Result<T, E>` or evaluate a fallback.
//!
//! Reached from the `lower_expr_uncoerced` dispatch in this module's `mod.rs`.
//! Every file here adds methods to the same `impl Lowerer` block.

use ast_types::Expr;
use neuro_hir::{
    HirBindingSource, HirExpr, HirExprKind, HirMatchArm, HirMatchBinding, HirMatchTest, HirType,
};

use crate::{Lowerer, LoweringError};

/// The two fallible enums, each paired with the variant that carries a value.
/// A `Result`'s `Err` payload has no slot in the desugar — `??` discards it.
const FALLIBLE_ENUMS: &[(&str, &str)] = &[("Option", "Some"), ("Result", "Ok")];

impl Lowerer {
    /// Lower `lhs ?? fallback` into the `match` it is defined as:
    ///
    /// ```text
    /// match lhs {
    ///     <Success>(__coalesce_N) => __coalesce_N,
    ///     _                       => fallback
    /// }
    /// ```
    ///
    /// Desugaring here rather than adding a HIR node is what makes the fallback lazy for
    /// free: the backend already emits one basic block per arm, so `fallback` is only
    /// reached when the success test fails. Both backends stay unaware of `??`.
    pub(super) fn lower_null_coalesce(
        &mut self,
        left: &Expr,
        right: &Expr,
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        let scrutinee = self.lower_expr(left, None)?;
        let (tag, payload) = self.success_variant(&scrutinee.ty)?;

        let binding = format!("__coalesce_{}", self.coalesce_counter);
        self.coalesce_counter += 1;

        let unwrapped = HirExpr::new(
            HirExprKind::Variable(binding.clone()),
            payload.clone(),
            span,
        );
        let fallback = self.lower_expr(right, Some(&payload))?;

        Ok(HirExpr::new(
            HirExprKind::Match {
                scrutinee: Box::new(scrutinee),
                arms: vec![
                    HirMatchArm {
                        tests: vec![HirMatchTest::Tag { tag }],
                        bindings: vec![HirMatchBinding {
                            name: binding,
                            ty: payload.clone(),
                            source: HirBindingSource::EnumPayload { slot: 0 },
                        }],
                        guard: None,
                        body: unwrapped,
                    },
                    HirMatchArm {
                        tests: vec![HirMatchTest::Wildcard],
                        bindings: Vec::new(),
                        guard: None,
                        body: fallback,
                    },
                ],
            },
            payload,
            span,
        ))
    }

    /// The prelude enum a fallible value is an instance of — `Option` or `Result`.
    ///
    /// A monomorphized `Option<i32>` answers with the template it came from; a program
    /// that shadows the prelude with a non-generic `Option` is its own base.
    pub(super) fn fallible_base(&self, ty: &HirType) -> Option<&str> {
        let HirType::Enum(instance) = ty.referent() else {
            return None;
        };
        let base = self
            .enum_instance_base
            .get(instance)
            .map(String::as_str)
            .unwrap_or(instance.as_str());
        FALLIBLE_ENUMS
            .iter()
            .find(|(name, _)| *name == base)
            .map(|(name, _)| *name)
    }

    /// The success variant's tag and payload type for a fallible enum instance.
    ///
    /// The checker has already rejected every other left operand, so a miss here means
    /// the two passes disagree — reported as a lowering error rather than a panic.
    pub(super) fn success_variant(&self, ty: &HirType) -> Result<(u32, HirType), LoweringError> {
        let HirType::Enum(instance) = ty.referent() else {
            return Err(Self::not_fallible(ty));
        };
        let Some(base) = self.fallible_base(ty) else {
            return Err(Self::not_fallible(ty));
        };
        let Some((_, variant)) = FALLIBLE_ENUMS.iter().find(|(name, _)| *name == base) else {
            return Err(Self::not_fallible(ty));
        };

        let (tag, fields) = self.enum_variant(instance, variant)?;
        let payload =
            fields
                .first()
                .map(|(_, t)| t.clone())
                .ok_or_else(|| LoweringError::Malformed {
                    detail: format!("`{}::{}` carries no payload to unwrap", base, variant),
                })?;
        Ok((tag, payload))
    }

    pub(super) fn not_fallible(ty: &HirType) -> LoweringError {
        LoweringError::Malformed {
            detail: format!("{:?} is not an Option or a Result", ty),
        }
    }
}
