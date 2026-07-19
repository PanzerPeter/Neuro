// Type checking for `match` expressions: pattern/scrutinee typing, arm-body
// unification, binding introduction, and exhaustiveness.

use ast_types::{EnumPatternPayload, Expr, MatchArm, Pattern};
use shared_types::{IntSuffix, Literal, Span};

use super::{TypeChecker, VariantForm};
use crate::errors::TypeError;
use crate::types::Type;

/// What a single guardless pattern proves about coverage of the scrutinee.
enum Coverage {
    /// Matches every value (`_` or a bare binding).
    CatchAll,
    /// Covers one enum variant by name.
    Variant(String),
    /// Covers one boolean value.
    Bool(bool),
    /// Contributes nothing decidable (a literal or range over an unbounded scalar).
    Nothing,
}

impl TypeChecker {
    /// Type-check a `match` expression and return its result type.
    pub(crate) fn check_match(
        &mut self,
        scrutinee: &Expr,
        arms: &[MatchArm],
        span: Span,
        expected: Option<&Type>,
    ) -> Type {
        let scrut_ty = self.check_expr(scrutinee, None).unwrap_or(Type::Unknown);

        // Only Copy scalars and enums are matchable in this phase; reject anything
        // else up front (but keep checking arms so their bodies still get diagnosed).
        let matchable = matches!(&scrut_ty, Type::Enum(_))
            || scrut_ty.is_integer()
            || scrut_ty.is_char()
            || scrut_ty.is_bool()
            || matches!(scrut_ty, Type::Unknown);
        if !matchable {
            self.record_error(TypeError::UnsupportedMatchScrutinee {
                ty: scrut_ty.clone(),
                span,
            });
        }

        // Each arm runs on its own path, so snapshot the move state after the
        // scrutinee and restore it between arms — mirroring `if`.
        let move_snapshot = self.symbols.snapshot_moves();

        // The body-type hint: the caller's expected type if any, else the first arm's
        // type once known, so `_ => 0` infers to a sibling arm's integer width.
        let mut hint: Option<Type> = expected.cloned();
        let mut arm_types: Vec<Type> = Vec::with_capacity(arms.len());
        for arm in arms {
            self.symbols.restore_moves(&move_snapshot);
            let arm_ty = self.check_arm(arm, &scrut_ty, hint.as_ref());
            if hint.is_none() && !matches!(arm_ty, Type::Unknown) {
                hint = Some(arm_ty.clone());
            }
            arm_types.push(arm_ty);
        }
        self.symbols.restore_moves(&move_snapshot);

        if matchable && !matches!(scrut_ty, Type::Unknown) {
            self.check_exhaustive(arms, &scrut_ty, span);
        }

        // Unify arm body types, mirroring the `if`-expression rule.
        let Some((first, rest)) = arm_types.split_first() else {
            return Type::Void;
        };
        let result_ty = first.clone();
        for arm_ty in rest {
            if !arm_ty.is_compatible_with(&result_ty) {
                self.record_error(TypeError::MatchArmTypeMismatch {
                    expected: result_ty.clone(),
                    found: arm_ty.clone(),
                    span,
                });
                return Type::Unknown;
            }
        }
        result_ty
    }

    /// Check one arm: its patterns, guard, and body. Introduces the pattern bindings
    /// into a fresh scope for the guard and body. Returns the body's type.
    fn check_arm(&mut self, arm: &MatchArm, scrut_ty: &Type, expected: Option<&Type>) -> Type {
        self.symbols.push_scope();

        // Or-patterns (`a | b`) may not bind: only a single pattern arm binds.
        let binds_allowed = arm.patterns.len() == 1;
        let mut bindings: Vec<(String, Type, Span)> = Vec::new();
        for pattern in &arm.patterns {
            let mut pat_bindings = Vec::new();
            self.check_pattern(pattern, scrut_ty, &mut pat_bindings);
            if !binds_allowed && !pat_bindings.is_empty() {
                self.record_error(TypeError::OrPatternBinding {
                    span: pattern.span(),
                });
            } else if binds_allowed {
                bindings = pat_bindings;
            }
        }

        for (name, ty, _span) in &bindings {
            let _ = self.symbols.define(name.clone(), ty.clone(), false);
        }

        if let Some(guard) = &arm.guard {
            let guard_ty = self
                .check_expr(guard, Some(&Type::Bool))
                .unwrap_or(Type::Unknown);
            if !matches!(guard_ty, Type::Unknown) && !guard_ty.is_bool() {
                self.record_error(TypeError::Mismatch {
                    expected: Type::Bool,
                    found: guard_ty,
                    span: guard.span(),
                });
            }
        }

        let body_ty = self
            .check_expr(&arm.body, expected)
            .unwrap_or(Type::Unknown);
        self.symbols.pop_scope();
        body_ty
    }

    /// Check a pattern against the scrutinee type, collecting the bindings it
    /// introduces into `bindings`.
    fn check_pattern(
        &mut self,
        pattern: &Pattern,
        scrut_ty: &Type,
        bindings: &mut Vec<(String, Type, Span)>,
    ) {
        match pattern {
            Pattern::Wildcard(_) => {}
            Pattern::Binding(ident) => {
                bindings.push((ident.name.clone(), scrut_ty.clone(), ident.span));
            }
            Pattern::Literal(lit, span) => {
                self.check_literal_pattern(lit, scrut_ty, *span);
            }
            Pattern::Range {
                start, end, span, ..
            } => {
                if !(scrut_ty.is_integer()
                    || scrut_ty.is_char()
                    || matches!(scrut_ty, Type::Unknown))
                {
                    self.record_error(TypeError::InvalidRangePattern { span: *span });
                    return;
                }
                self.check_literal_pattern(start, scrut_ty, *span);
                self.check_literal_pattern(end, scrut_ty, *span);
            }
            Pattern::Enum {
                enum_name,
                variant,
                payload,
                span,
            } => self.check_enum_pattern(
                enum_name.name.as_str(),
                &variant.name,
                payload,
                scrut_ty,
                *span,
                bindings,
            ),
        }
    }

    /// Check an enum-variant pattern and bind its payload sub-patterns.
    fn check_enum_pattern(
        &mut self,
        enum_name: &str,
        variant: &str,
        payload: &EnumPatternPayload,
        scrut_ty: &Type,
        span: Span,
        bindings: &mut Vec<(String, Type, Span)>,
    ) {
        // The scrutinee must be exactly this enum (nominal typing).
        match scrut_ty {
            Type::Unknown => {}
            Type::Enum(name) if name == enum_name => {}
            _ => {
                self.record_error(TypeError::PatternTypeMismatch {
                    pattern_ty: format!("{}::{}", enum_name, variant),
                    scrutinee_ty: scrut_ty.clone(),
                    span,
                });
                return;
            }
        }

        let Some(info) = self.lookup_enum_variant(enum_name, variant) else {
            self.record_error(TypeError::UnknownEnumVariant {
                enum_name: enum_name.to_string(),
                variant: variant.to_string(),
                span,
            });
            return;
        };
        let form = info.form;
        // Clone the field list so the immutable borrow of `self` ends before we bind.
        let fields: Vec<(Option<String>, Type)> = info.fields.clone();

        match (form, payload) {
            (VariantForm::Unit, EnumPatternPayload::Unit) => {}
            (VariantForm::Tuple, EnumPatternPayload::Tuple(subs)) => {
                if subs.len() != fields.len() {
                    self.record_error(TypeError::EnumVariantArityMismatch {
                        enum_name: enum_name.to_string(),
                        variant: variant.to_string(),
                        expected: fields.len(),
                        found: subs.len(),
                        span,
                    });
                }
                for (sub, (_, field_ty)) in subs.iter().zip(fields.iter()) {
                    self.bind_payload_sub(sub, field_ty, bindings);
                }
            }
            (VariantForm::Struct, EnumPatternPayload::Struct(field_pats)) => {
                for fp in field_pats {
                    match fields
                        .iter()
                        .find(|(n, _)| n.as_deref() == Some(fp.field.name.as_str()))
                    {
                        Some((_, field_ty)) => {
                            self.bind_payload_sub(&fp.pattern, field_ty, bindings)
                        }
                        None => self.record_error(TypeError::UnknownEnumField {
                            enum_name: enum_name.to_string(),
                            variant: variant.to_string(),
                            field: fp.field.name.clone(),
                            span: fp.span,
                        }),
                    }
                }
            }
            (expected_form, _) => {
                self.record_error(TypeError::VariantPatternFormMismatch {
                    enum_name: enum_name.to_string(),
                    variant: variant.to_string(),
                    expected: variant_form_word(expected_form).to_string(),
                    span,
                });
            }
        }
    }

    /// Bind one enum-payload sub-pattern. Sub-patterns are restricted to a binding or
    /// `_`; a refutable sub-pattern (literal, range, nested variant) is an
    /// error directing the user to a guard.
    fn bind_payload_sub(
        &mut self,
        sub: &Pattern,
        field_ty: &Type,
        bindings: &mut Vec<(String, Type, Span)>,
    ) {
        match sub {
            Pattern::Wildcard(_) => {}
            Pattern::Binding(ident) => {
                bindings.push((ident.name.clone(), field_ty.clone(), ident.span));
            }
            other => self.record_error(TypeError::RefutablePayloadPattern { span: other.span() }),
        }
    }

    /// Check that a literal pattern's type is compatible with the scrutinee.
    fn check_literal_pattern(&mut self, lit: &Literal, scrut_ty: &Type, span: Span) {
        if matches!(scrut_ty, Type::Unknown) {
            return;
        }
        let ok = match lit {
            Literal::Integer(_, None) => scrut_ty.is_integer(),
            Literal::Integer(_, Some(suffix)) => &int_suffix_type(suffix) == scrut_ty,
            Literal::Char(_) => scrut_ty.is_char(),
            Literal::Boolean(_) => scrut_ty.is_bool(),
            // Float and string literal patterns have no matchable scrutinee in phase 1E.
            Literal::Float(_, _) | Literal::String(_) => false,
        };
        if !ok {
            self.record_error(TypeError::PatternTypeMismatch {
                pattern_ty: literal_type_word(lit).to_string(),
                scrutinee_ty: scrut_ty.clone(),
                span,
            });
        }
    }

    /// Verify the arms cover every possible scrutinee value.
    fn check_exhaustive(&mut self, arms: &[MatchArm], scrut_ty: &Type, span: Span) {
        let mut has_catch_all = false;
        let mut covered_variants: Vec<String> = Vec::new();
        let mut bools_covered = [false; 2];

        for arm in arms {
            // A guarded arm may not fire, so it never contributes to exhaustiveness.
            if arm.guard.is_some() {
                continue;
            }
            for pat in &arm.patterns {
                match pattern_coverage(pat) {
                    Coverage::CatchAll => has_catch_all = true,
                    Coverage::Variant(name) => {
                        if !covered_variants.contains(&name) {
                            covered_variants.push(name);
                        }
                    }
                    Coverage::Bool(b) => bools_covered[b as usize] = true,
                    Coverage::Nothing => {}
                }
            }
        }

        if has_catch_all {
            return;
        }

        match scrut_ty {
            Type::Enum(name) => {
                let all: Vec<String> = self
                    .enum_defs
                    .get(name)
                    .map(|vs| vs.iter().map(|v| v.name.clone()).collect())
                    .unwrap_or_default();
                let missing: Vec<String> = all
                    .into_iter()
                    .filter(|v| !covered_variants.contains(v))
                    .collect();
                if !missing.is_empty() {
                    self.record_error(TypeError::NonExhaustiveMatch {
                        reason: format!("unhandled variant(s): {}", missing.join(", ")),
                        span,
                    });
                }
            }
            Type::Bool => {
                if !(bools_covered[0] && bools_covered[1]) {
                    self.record_error(TypeError::NonExhaustiveMatch {
                        reason: "not all `bool` values are handled".to_string(),
                        span,
                    });
                }
            }
            // Integers and `char` have too many values to enumerate; they require a
            // wildcard arm.
            _ => {
                self.record_error(TypeError::NonExhaustiveMatch {
                    reason: format!("a `{}` match requires a `_` wildcard arm", scrut_ty),
                    span,
                });
            }
        }
    }
}

/// The coverage a single guardless pattern contributes to exhaustiveness.
fn pattern_coverage(pat: &Pattern) -> Coverage {
    match pat {
        Pattern::Wildcard(_) | Pattern::Binding(_) => Coverage::CatchAll,
        Pattern::Enum { variant, .. } => Coverage::Variant(variant.name.clone()),
        Pattern::Literal(Literal::Boolean(b), _) => Coverage::Bool(*b),
        Pattern::Literal(_, _) | Pattern::Range { .. } => Coverage::Nothing,
    }
}

/// The human-readable word for a variant form, for the form-mismatch diagnostic.
fn variant_form_word(form: VariantForm) -> &'static str {
    match form {
        VariantForm::Unit => "unit",
        VariantForm::Tuple => "tuple",
        VariantForm::Struct => "struct",
    }
}

/// A short description of a literal's type family, for pattern-mismatch diagnostics.
fn literal_type_word(lit: &Literal) -> &'static str {
    match lit {
        Literal::Integer(_, _) => "an integer",
        Literal::Float(_, _) => "a float",
        Literal::Char(_) => "a `char`",
        Literal::Boolean(_) => "a `bool`",
        Literal::String(_) => "a `string`",
    }
}

/// The concrete integer type an integer-literal suffix denotes.
fn int_suffix_type(suffix: &IntSuffix) -> Type {
    match suffix {
        IntSuffix::I8 => Type::I8,
        IntSuffix::I16 => Type::I16,
        IntSuffix::I32 => Type::I32,
        IntSuffix::I64 => Type::I64,
        IntSuffix::U8 => Type::U8,
        IntSuffix::U16 => Type::U16,
        IntSuffix::U32 => Type::U32,
        IntSuffix::U64 => Type::U64,
    }
}
