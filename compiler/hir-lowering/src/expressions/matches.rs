//! Pattern matching: `match` lowering and the tests and bindings each arm needs.
//!
//! Reached from the `lower_expr_uncoerced` dispatch in this module's `mod.rs`.
//! Every file here adds methods to the same `impl Lowerer` block.

use ast_types::Expr;
use neuro_hir::{HirExpr, HirExprKind, HirType};

use super::literal_scalar;
use crate::{Lowerer, LoweringError};

impl Lowerer {
    /// Lower a `match` expression into the fully-resolved HIR node: each arm's
    /// patterns become refutable tests, its bindings resolve to payload slots or the
    /// whole scrutinee, and the guard/body lower with the bindings in scope. The match
    /// type is the first arm's body type, mirroring the checker.
    pub(super) fn lower_match(
        &mut self,
        scrutinee: &Expr,
        arms: &[ast_types::MatchArm],
        expected: Option<&HirType>,
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        let scrutinee = self.lower_expr(scrutinee, None)?;
        let scrut_ty = scrutinee.ty.clone();

        // Body-type hint, mirroring the checker: the expected type if any, else the
        // first arm's type, so a later `_ => 0` infers a sibling arm's integer width.
        let mut hint: Option<HirType> = expected.cloned();
        let mut hir_arms = Vec::with_capacity(arms.len());
        let mut result_ty = HirType::Void;
        for (i, arm) in arms.iter().enumerate() {
            let mut tests = Vec::with_capacity(arm.patterns.len());
            for pat in &arm.patterns {
                tests.push(self.pattern_test(pat, &scrut_ty)?);
            }
            // Only a single-pattern arm binds (or-patterns cannot).
            let bindings = if arm.patterns.len() == 1 {
                self.pattern_bindings(&arm.patterns[0], &scrut_ty)?
            } else {
                Vec::new()
            };

            self.push_scope();
            for b in &bindings {
                self.define(b.name.clone(), b.ty.clone());
            }
            let guard = match &arm.guard {
                Some(g) => Some(self.lower_expr(g, Some(&HirType::Bool))?),
                None => None,
            };
            let body = self.lower_expr(&arm.body, hint.as_ref())?;
            self.pop_scope();

            if hint.is_none() {
                hint = Some(body.ty.clone());
            }
            if i == 0 {
                result_ty = body.ty.clone();
            }
            hir_arms.push(neuro_hir::HirMatchArm {
                tests,
                bindings,
                guard,
                body,
            });
        }

        Ok(HirExpr::new(
            HirExprKind::Match {
                scrutinee: Box::new(scrutinee),
                arms: hir_arms,
            },
            result_ty,
            span,
        ))
    }

    /// The enum an enum pattern resolves against: the scrutinee's own type when the
    /// pattern names the generic base it was monomorphized from, else the pattern's name.
    pub(super) fn pattern_enum_name(&self, base: &str, scrut_ty: &HirType) -> String {
        match scrut_ty {
            HirType::Enum(name)
                if self.enum_instance_base.get(name).map(|s| s.as_str()) == Some(base) =>
            {
                name.clone()
            }
            _ => base.to_string(),
        }
    }

    /// Build the refutable [`HirMatchTest`] for one pattern.
    pub(crate) fn pattern_test(
        &self,
        pat: &ast_types::Pattern,
        scrut_ty: &HirType,
    ) -> Result<neuro_hir::HirMatchTest, LoweringError> {
        use neuro_hir::HirMatchTest;
        match pat {
            ast_types::Pattern::Wildcard(_) | ast_types::Pattern::Binding(_) => {
                Ok(HirMatchTest::Wildcard)
            }
            ast_types::Pattern::Literal(lit, _) => Ok(HirMatchTest::IntEq {
                value: literal_scalar(lit)?,
            }),
            ast_types::Pattern::Range {
                start,
                end,
                inclusive,
                ..
            } => {
                let lo = literal_scalar(start)?;
                let hi_raw = literal_scalar(end)?;
                // Normalize an exclusive `a..b` to the inclusive `a..=b-1` codegen uses.
                let hi = if *inclusive { hi_raw } else { hi_raw - 1 };
                Ok(HirMatchTest::IntRange { lo, hi })
            }
            ast_types::Pattern::Enum {
                enum_name, variant, ..
            } => {
                let resolved = self.pattern_enum_name(&enum_name.name, scrut_ty);
                let (tag, _) = self.enum_variant(&resolved, &variant.name)?;
                Ok(HirMatchTest::Tag { tag })
            }
            // Module resolution rewrites an imported variant into `Pattern::Enum`, and the
            // checker rejects any it could not; reaching lowering means neither ran.
            ast_types::Pattern::UnqualifiedEnum { variant, .. } => Err(LoweringError::Malformed {
                detail: format!("variant `{}` was never resolved to an enum", variant.name),
            }),
        }
    }

    /// Resolve the bindings a single (non-or) pattern introduces: the whole
    /// scrutinee for a bare binding, or payload slot extractions for an enum pattern.
    pub(crate) fn pattern_bindings(
        &self,
        pat: &ast_types::Pattern,
        scrut_ty: &HirType,
    ) -> Result<Vec<neuro_hir::HirMatchBinding>, LoweringError> {
        use neuro_hir::{HirBindingSource, HirMatchBinding};
        match pat {
            ast_types::Pattern::Binding(ident) => Ok(vec![HirMatchBinding {
                name: ident.name.clone(),
                ty: scrut_ty.clone(),
                source: HirBindingSource::Scrutinee,
            }]),
            ast_types::Pattern::Enum {
                enum_name,
                variant,
                payload,
                ..
            } => {
                let resolved = self.pattern_enum_name(&enum_name.name, scrut_ty);
                let (_, fields) = self.enum_variant(&resolved, &variant.name)?;
                let mut bindings = Vec::new();
                match payload {
                    ast_types::EnumPatternPayload::Unit => {}
                    ast_types::EnumPatternPayload::Tuple(subs) => {
                        for (slot, sub) in subs.iter().enumerate() {
                            if let ast_types::Pattern::Binding(ident) = sub {
                                let ty = fields
                                    .get(slot)
                                    .map(|(_, t)| t.clone())
                                    .unwrap_or(HirType::Void);
                                bindings.push(HirMatchBinding {
                                    name: ident.name.clone(),
                                    ty,
                                    source: HirBindingSource::EnumPayload { slot },
                                });
                            }
                        }
                    }
                    ast_types::EnumPatternPayload::Struct(field_pats) => {
                        for fp in field_pats {
                            if let ast_types::Pattern::Binding(ident) = &fp.pattern {
                                let slot = fields
                                    .iter()
                                    .position(|(n, _)| n.as_deref() == Some(fp.field.name.as_str()))
                                    .ok_or_else(|| LoweringError::Malformed {
                                        detail: format!(
                                            "unknown field '{}' in variant '{}::{}' pattern",
                                            fp.field.name, enum_name.name, variant.name
                                        ),
                                    })?;
                                let ty = fields[slot].1.clone();
                                bindings.push(HirMatchBinding {
                                    name: ident.name.clone(),
                                    ty,
                                    source: HirBindingSource::EnumPayload { slot },
                                });
                            }
                        }
                    }
                }
                Ok(bindings)
            }
            _ => Ok(Vec::new()),
        }
    }
}
