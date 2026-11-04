//! Pattern matching: `match` lowering and the tests and bindings each arm needs.
//!
//! Reached from the `lower_expr_uncoerced` dispatch in this module's `mod.rs`.
//! Every file here adds methods to the same `impl Lowerer` block.

use ast_types::{ArrayPatternRest, EnumPatternPayload, Expr, Pattern};
use neuro_hir::{HirExpr, HirExprKind, HirMatchBinding, HirMatchTest, HirPatternStep, HirType};

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
        // A borrowed scrutinee is matched through: the patterns test its referent. A
        // slice has no value of its own, so it stays the borrow.
        let scrutinee = match scrutinee.ty.clone() {
            HirType::Reference { inner, .. } if !matches!(*inner, HirType::Slice(_)) => {
                let span = scrutinee.span;
                HirExpr::new(
                    HirExprKind::Deref {
                        operand: Box::new(scrutinee),
                    },
                    *inner,
                    span,
                )
            }
            _ => scrutinee,
        };
        let scrut_ty = scrutinee.ty.clone();

        // Body-type hint, mirroring the checker: the expected type if any, else the
        // first arm's type, so a later `_ => 0` infers a sibling arm's integer width.
        let mut hint: Option<HirType> = expected.cloned();
        let mut hir_arms = Vec::with_capacity(arms.len());
        let mut result_ty = HirType::Void;
        for arm in arms {
            let (tests, bindings) = self.lower_arm_patterns(&arm.patterns, &scrut_ty)?;

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
            // The first arm that carries a type decides the match's, mirroring the
            // checker. A divergent arm (a `panic` or an `unreachable` with no context
            // type) lowers to `void` and describes nothing, so taking it made the whole
            // match void purely because of the order the arms were written in.
            if matches!(result_ty, HirType::Void) {
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

    /// Lower an arm's alternatives. Each binding gathers one path per alternative, in the
    /// first alternative's order; the checker has proved they all bind the same names.
    fn lower_arm_patterns(
        &self,
        patterns: &[Pattern],
        scrut_ty: &HirType,
    ) -> Result<(Vec<HirMatchTest>, Vec<HirMatchBinding>), LoweringError> {
        let mut tests = Vec::with_capacity(patterns.len());
        let mut bindings: Vec<HirMatchBinding> = Vec::new();
        for (i, pattern) in patterns.iter().enumerate() {
            let (test, found) = self.lower_pattern(pattern, scrut_ty)?;
            tests.push(test);
            if i == 0 {
                bindings = found;
                continue;
            }
            for binding in &mut bindings {
                let same = found.iter().find(|b| b.name == binding.name);
                let same = same.ok_or_else(|| LoweringError::Malformed {
                    detail: format!("alternative does not bind '{}'", binding.name),
                })?;
                binding.paths.extend(same.paths.iter().cloned());
            }
        }
        Ok((tests, bindings))
    }

    /// Lower one pattern to the test that decides it and the bindings it introduces.
    pub(crate) fn lower_pattern(
        &self,
        pat: &Pattern,
        scrut_ty: &HirType,
    ) -> Result<(HirMatchTest, Vec<HirMatchBinding>), LoweringError> {
        let mut bindings = Vec::new();
        let test = self.pattern_at(pat, scrut_ty, &mut Vec::new(), &mut bindings)?;
        Ok((test, bindings))
    }

    /// The test for `pat` read at `ty`, the type of the part `path` reaches.
    fn pattern_at(
        &self,
        pat: &Pattern,
        ty: &HirType,
        path: &mut Vec<HirPatternStep>,
        bindings: &mut Vec<HirMatchBinding>,
    ) -> Result<HirMatchTest, LoweringError> {
        let bind = |name: &str, ty: &HirType, path: &[HirPatternStep]| HirMatchBinding {
            name: name.to_string(),
            ty: ty.clone(),
            paths: vec![path.to_vec()],
        };
        // A pattern that looks inside a value looks through a borrow of it.
        if let HirType::Reference { inner, .. } = ty
            && !matches!(**inner, HirType::Slice(_))
            && !matches!(
                pat,
                Pattern::Wildcard(_) | Pattern::Binding(_) | Pattern::At { .. }
            )
        {
            let step = HirPatternStep::Deref {
                ty: (**inner).clone(),
            };
            return self.part_test(pat, inner, step, path, bindings);
        }
        match pat {
            Pattern::Wildcard(_) => Ok(HirMatchTest::Wildcard),
            Pattern::Binding(ident) => {
                bindings.push(bind(&ident.name, ty, path));
                Ok(HirMatchTest::Wildcard)
            }
            Pattern::At { name, pattern, .. } => {
                bindings.push(bind(&name.name, ty, path));
                self.pattern_at(pattern, ty, path, bindings)
            }
            Pattern::Literal(lit, _) => Ok(HirMatchTest::IntEq {
                value: literal_scalar(lit)?,
            }),
            Pattern::Range {
                start,
                end,
                inclusive,
                ..
            } => {
                let lo = literal_scalar(start)?;
                let hi_raw = literal_scalar(end)?;
                // Normalize an exclusive `a..b` to the inclusive `a..=b-1` codegen uses.
                // The bounds are the low 64 bits of the literal, so a `u64` end of `2^63`
                // arrives as `i64::MIN`, and the step down must wrap to keep the bits.
                let hi = if *inclusive {
                    hi_raw
                } else {
                    hi_raw.wrapping_sub(1)
                };
                Ok(HirMatchTest::IntRange { lo, hi })
            }
            Pattern::Enum {
                enum_name,
                variant,
                payload,
                ..
            } => {
                let resolved = self.pattern_enum_name(&enum_name.name, ty);
                let (tag, fields) = self.enum_variant(&resolved, &variant.name)?;
                let mut tests = vec![HirMatchTest::Tag { tag }];
                let slot_of = |name: &str| {
                    let slot = fields.iter().position(|(n, _)| n.as_deref() == Some(name));
                    slot.ok_or_else(|| LoweringError::Malformed {
                        detail: format!(
                            "unknown field '{name}' in variant '{}::{}' pattern",
                            enum_name.name, variant.name
                        ),
                    })
                };
                let subs: Vec<(usize, &Pattern)> = match payload {
                    EnumPatternPayload::Unit => Vec::new(),
                    EnumPatternPayload::Tuple(subs) => subs.iter().enumerate().collect(),
                    EnumPatternPayload::Struct(field_pats) => field_pats
                        .iter()
                        .map(|fp| Ok((slot_of(&fp.field.name)?, &fp.pattern)))
                        .collect::<Result<_, LoweringError>>()?,
                };
                for (slot, sub) in subs {
                    let field_ty = fields.get(slot).map_or(HirType::Void, |(_, t)| t.clone());
                    let step = HirPatternStep::EnumPayload {
                        slot,
                        ty: field_ty.clone(),
                    };
                    tests.push(self.part_test(sub, &field_ty, step, path, bindings)?);
                }
                Ok(all_tests(tests))
            }
            Pattern::Array { elements, rest, .. } => {
                self.array_pattern(elements, rest.as_ref(), ty, path, bindings)
            }
            // Module resolution rewrites an imported variant into `Pattern::Enum`, and the
            // checker rejects any it could not; reaching lowering means neither ran.
            Pattern::UnqualifiedEnum { variant, .. } => Err(LoweringError::Malformed {
                detail: format!("variant `{}` was never resolved to an enum", variant.name),
            }),
        }
    }

    /// `pat` read one `step` further along `path`.
    fn part_test(
        &self,
        pat: &Pattern,
        ty: &HirType,
        step: HirPatternStep,
        path: &mut Vec<HirPatternStep>,
        bindings: &mut Vec<HirMatchBinding>,
    ) -> Result<HirMatchTest, LoweringError> {
        path.push(step.clone());
        let test = self.pattern_at(pat, ty, path, bindings);
        path.pop();
        Ok(match test? {
            HirMatchTest::Wildcard => HirMatchTest::Wildcard,
            test => HirMatchTest::Part {
                step,
                test: Box::new(test),
            },
        })
    }

    fn array_pattern(
        &self,
        elements: &[Pattern],
        rest: Option<&ArrayPatternRest>,
        ty: &HirType,
        path: &mut Vec<HirPatternStep>,
        bindings: &mut Vec<HirMatchBinding>,
    ) -> Result<HirMatchTest, LoweringError> {
        let bound = elements.len();
        let mut tests = Vec::new();
        let (element, rest_ty) = match ty {
            HirType::Array { element, size } => {
                // Only a size parameter gets here with a length that differs: the checker
                // refuses it for a written size, and the template's arms cover every `N`.
                if bound > *size || (rest.is_none() && bound != *size) {
                    tests.push(HirMatchTest::Never);
                }
                let rest_ty = HirType::Array {
                    element: element.clone(),
                    size: size.saturating_sub(bound),
                };
                (element.as_ref(), rest_ty)
            }
            HirType::Reference { inner, .. } => {
                let HirType::Slice(element) = inner.as_ref() else {
                    return Err(LoweringError::Malformed {
                        detail: format!("an array pattern over `{ty}`"),
                    });
                };
                tests.push(HirMatchTest::SliceLen {
                    len: bound as u64,
                    exact: rest.is_none(),
                });
                let rest_ty = HirType::Reference {
                    inner: inner.clone(),
                    mutable: false,
                };
                (element.as_ref(), rest_ty)
            }
            _ => {
                return Err(LoweringError::Malformed {
                    detail: format!("an array pattern of {bound} element(s) over `{ty}`"),
                });
            }
        };
        for (index, sub) in elements.iter().enumerate() {
            let step = HirPatternStep::Element {
                index: index as u64,
                ty: element.clone(),
            };
            tests.push(self.part_test(sub, element, step, path, bindings)?);
        }
        if let Some(ident) = rest.and_then(|r| r.binding.as_ref()) {
            let mut rest_path = path.clone();
            rest_path.push(HirPatternStep::Rest {
                start: bound as u64,
                ty: rest_ty.clone(),
            });
            bindings.push(HirMatchBinding {
                name: ident.name.clone(),
                ty: rest_ty,
                paths: vec![rest_path],
            });
        }
        Ok(all_tests(tests))
    }
}

/// The conjunction of `tests`, left to right. Wildcards say nothing and drop out.
fn all_tests(tests: Vec<HirMatchTest>) -> HirMatchTest {
    let mut tests: Vec<HirMatchTest> = tests
        .into_iter()
        .filter(|test| !matches!(test, HirMatchTest::Wildcard))
        .collect();
    match tests.len() {
        0 => HirMatchTest::Wildcard,
        1 => tests.remove(0),
        _ => HirMatchTest::All(tests),
    }
}
