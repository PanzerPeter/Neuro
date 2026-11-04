// Type checking for `match` expressions: pattern/scrutinee typing, arm-body
// unification and binding introduction. Exhaustiveness is `exhaustiveness.rs`.

use ast_types::{ArrayPatternRest, EnumPatternPayload, Expr, MatchArm, Pattern};
use shared_types::{Literal, Span};

use super::literals::suffix_to_type;
use super::{TypeChecker, VariantForm};
use crate::errors::TypeError;
use crate::type_checkers::val_else::expr_diverges;
use crate::types::{ArrayLen, Type};

impl TypeChecker {
    /// Type-check a `match` expression and return its result type.
    pub(crate) fn check_match(
        &mut self,
        scrutinee: &Expr,
        arms: &[MatchArm],
        span: Span,
        expected: Option<&Type>,
    ) -> Type {
        let written_ty = self.check_expr(scrutinee, None).unwrap_or(Type::Unknown);
        // A borrow is matched through, as `self` is in a `&self` method and as `*r` is
        // written out: the patterns test the referent. A slice has no value of its own,
        // so it stays the borrow.
        let scrut_ty = if is_slice_borrow(&written_ty) {
            written_ty.clone()
        } else {
            written_ty.referent().clone()
        };

        // Reject anything else up front (but keep checking arms so their bodies still
        // get diagnosed).
        let matchable = matches!(&scrut_ty, Type::Enum(_) | Type::Array { .. })
            || is_slice_borrow(&scrut_ty)
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

        // An unmatchable scrutinee has been reported once. Checking the patterns
        // against its real type only restates that in worse words: a `string`
        // literal pattern would read "matches a `string` but the value has type
        // string", so the arms see `Unknown`, which every pattern accepts.
        let pattern_ty = if matchable {
            scrut_ty.clone()
        } else {
            Type::Unknown
        };

        // Each arm runs on its own path, so snapshot the move state after the
        // scrutinee and restore it between arms, then join in the moves of every arm
        // that falls through, mirroring `if`.
        let move_snapshot = self.symbols.snapshot_moves();
        let mut fell_through = Vec::new();
        // A transient borrow an arm takes ends with the arm as far as its siblings are
        // concerned: only one arm runs. Past the `match` the statement still holds what
        // the most borrowing arm took.
        let borrow_snapshot = self.symbols.transient_borrows();
        let mut arm_borrows = borrow_snapshot.clone();

        // The body-type hint: the caller's expected type if any, else the first arm's
        // type once known, so `_ => 0` infers to a sibling arm's integer width.
        let mut hint: Option<Type> = expected.cloned();
        let mut arm_types: Vec<Type> = Vec::with_capacity(arms.len());
        let mut takes_owner = false;
        for arm in arms {
            self.symbols.restore_moves(&move_snapshot);
            self.symbols.set_transient_borrows(&borrow_snapshot);
            let (arm_ty, binds_owner) = self.check_arm(arm, scrutinee, &pattern_ty, hint.as_ref());
            arm_borrows.extend(self.symbols.transient_borrows());
            if !expr_diverges(&arm.body) {
                fell_through.push(self.symbols.snapshot_moves());
            }
            takes_owner |= binds_owner;
            if hint.is_none() && !matches!(arm_ty, Type::Unknown) {
                hint = Some(arm_ty.clone());
            }
            arm_types.push(arm_ty);
        }
        self.symbols.restore_moves(&move_snapshot);
        self.symbols.settle_moves(&fell_through);
        self.symbols.set_transient_borrows(&arm_borrows);
        if takes_owner {
            self.take_from_scrutinee(scrutinee, &written_ty, span);
        }

        if matchable && !matches!(scrut_ty, Type::Unknown) {
            self.check_exhaustive(arms, &scrut_ty, span);
        }

        // Unify arm body types, mirroring the `if`-expression rule: the result is the
        // first arm that carries a type. A divergent arm (one that panics or leaves
        // the scope) is `Unknown`, compatible with everything, and describes nothing.
        if arm_types.is_empty() {
            return Type::Void;
        }
        let result_ty = arm_types
            .iter()
            .find(|ty| !matches!(ty, Type::Unknown))
            .cloned()
            .unwrap_or(Type::Unknown);
        for arm_ty in &arm_types {
            if !arm_ty.is_compatible_with(&result_ty) {
                self.record_error(TypeError::MatchArmTypeMismatch {
                    expected: result_ty,
                    found: arm_ty.clone(),
                    span,
                });
                return Type::Unknown;
            }
        }
        result_ty
    }

    /// Check one arm: its patterns, guard, and body. Introduces the pattern bindings
    /// into a fresh scope for the guard and body. Returns the body's type, and whether
    /// whether it binds a value that owns something, which the arm moves out.
    fn check_arm(
        &mut self,
        arm: &MatchArm,
        scrutinee: &Expr,
        scrut_ty: &Type,
        expected: Option<&Type>,
    ) -> (Type, bool) {
        self.symbols.push_scope();

        let mut bindings: Vec<(String, Type, Span)> = Vec::new();
        for (i, pattern) in arm.patterns.iter().enumerate() {
            let mut pat_bindings = Vec::new();
            self.check_pattern(pattern, scrut_ty, &mut pat_bindings);
            if i == 0 {
                bindings = pat_bindings;
            } else {
                self.check_alternative_bindings(&bindings, &pat_bindings, pattern.span());
            }
        }

        let binds_owner = bindings
            .iter()
            .any(|(_, ty, _)| self.is_type_move_tracked(ty));
        // The arm's scope is fresh, so a failed define is a name the pattern binds twice.
        // A binding is a part of the scrutinee, so it holds what the scrutinee borrows.
        for (name, ty, span) in &bindings {
            if let Err(duplicate) = self.symbols.define(name.clone(), ty.clone(), false) {
                self.record_error(TypeError::VariableAlreadyDefined {
                    name: duplicate,
                    span: *span,
                });
                continue;
            }
            self.hold_carried_borrows(name, scrutinee, ty);
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
        self.pop_scope();
        // An arm that leaves the enclosing scope never reaches the point where the
        // `match` has a value, so it says nothing about that value and must not
        // constrain its siblings: the contract `panic` / `unreachable` already have.
        if expr_diverges(&arm.body) {
            return (Type::Unknown, binds_owner);
        }
        (body_ty, binds_owner)
    }

    /// Every alternative of an arm feeds one body, so each binds what the first does.
    fn check_alternative_bindings(
        &mut self,
        first: &[(String, Type, Span)],
        other: &[(String, Type, Span)],
        span: Span,
    ) {
        let binds = |list: &[(String, Type, Span)], name: &str| {
            list.iter().filter(|(n, _, _)| n == name).count()
        };
        for (name, ty, _) in first {
            if binds(other, name) != binds(first, name) {
                self.record_error(TypeError::OrPatternBindingMissing {
                    name: name.clone(),
                    span,
                });
            } else if let Some((_, other_ty, _)) = other.iter().find(|(n, _, _)| n == name)
                && !other_ty.is_compatible_with(ty)
            {
                self.record_error(TypeError::OrPatternBindingType {
                    name: name.clone(),
                    first: ty.clone(),
                    other: other_ty.clone(),
                    span,
                });
            }
        }
        for (name, _, name_span) in other {
            if binds(first, name) == 0 {
                self.record_error(TypeError::OrPatternBindingMissing {
                    name: name.clone(),
                    span: *name_span,
                });
            }
        }
    }

    /// Settle what binding an owner by value does to the scrutinee. An owned scrutinee
    /// is given up whichever arm runs, as `??` and `?` give theirs up. A borrowed one
    /// (`self` in a `&self` method, or `*r`) owns nothing to give, so taking an owner out
    /// of it is refused, as `return self.field` is.
    fn take_from_scrutinee(&mut self, scrutinee: &Expr, scrut_ty: &Type, span: Span) {
        let behind_borrow = matches!(scrut_ty, Type::Reference { .. })
            || self
                .place_origin(scrutinee)
                .is_some_and(|(_, borrowed)| borrowed);
        if !behind_borrow {
            self.record_move(scrutinee);
            return;
        }
        let name = Self::place_root_name(scrutinee).unwrap_or_else(|| "value".to_string());
        self.record_error(TypeError::CannotMoveOutOfBorrow { name, span });
    }

    /// Check a pattern against the scrutinee type, collecting the bindings it
    /// introduces into `bindings`. A pattern that looks inside a value looks through a
    /// borrow of it, and nothing that owns may leave that borrow.
    pub(crate) fn check_pattern(
        &mut self,
        pattern: &Pattern,
        scrut_ty: &Type,
        bindings: &mut Vec<(String, Type, Span)>,
    ) {
        let looks_inside = !matches!(
            pattern,
            Pattern::Wildcard(_) | Pattern::Binding(_) | Pattern::At { .. }
        );
        if looks_inside
            && !is_slice_borrow(scrut_ty)
            && let Type::Reference { inner, .. } = scrut_ty
        {
            let first_new = bindings.len();
            self.check_pattern(pattern, inner, bindings);
            for (name, ty, span) in &bindings[first_new..] {
                if self.is_type_move_tracked(ty) {
                    self.record_error(TypeError::CannotMoveOutOfBorrow {
                        name: name.clone(),
                        span: *span,
                    });
                }
            }
            return;
        }
        match pattern {
            Pattern::Wildcard(_) => {}
            Pattern::Binding(ident) => {
                bindings.push((ident.name.clone(), scrut_ty.clone(), ident.span));
            }
            Pattern::At {
                name,
                pattern,
                span,
            } => {
                bindings.push((name.name.clone(), scrut_ty.clone(), name.span));
                let first_new = bindings.len();
                self.check_pattern(pattern, scrut_ty, bindings);
                if bindings.len() > first_new && self.is_type_move_tracked(scrut_ty) {
                    self.record_error(TypeError::AtPatternBindsOwner {
                        name: name.name.clone(),
                        span: *span,
                    });
                }
            }
            Pattern::Array {
                elements,
                rest,
                span,
            } => self.check_array_pattern(elements, rest.as_ref(), scrut_ty, *span, bindings),
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
            // Module resolution rewrites an imported variant into `Pattern::Enum` and
            // rejects any it cannot account for, so one arriving here means the pattern
            // names a variant no import brought into scope.
            Pattern::UnqualifiedEnum { variant, span, .. } => {
                self.record_error(TypeError::UnimportedVariantPattern {
                    variant: variant.name.clone(),
                    span: *span,
                });
            }
        }
    }

    /// Check `[p0, p1, ..rest]` against an array or a borrowed slice. An array's length
    /// is part of its type, so a pattern that cannot fit is an error, not a dead arm.
    fn check_array_pattern(
        &mut self,
        elements: &[Pattern],
        rest: Option<&ArrayPatternRest>,
        scrut_ty: &Type,
        span: Span,
        bindings: &mut Vec<(String, Type, Span)>,
    ) {
        let (element_ty, rest_ty) = match scrut_ty {
            Type::Array { element, size } => {
                let rest_ty = match size {
                    ArrayLen::Fixed(n) => {
                        let bound = elements.len();
                        if bound > *n || (rest.is_none() && bound != *n) {
                            self.record_error(TypeError::ArrayPatternLengthMismatch {
                                expected: bound,
                                found: *n,
                                span,
                            });
                        }
                        Type::Array {
                            element: element.clone(),
                            size: ArrayLen::Fixed(n.saturating_sub(bound)),
                        }
                    }
                    // A size parameter is settled per instance, as for a destructuring
                    // `val`.
                    _ => Type::Unknown,
                };
                ((**element).clone(), rest_ty)
            }
            Type::Reference { inner, .. } if is_slice_borrow(scrut_ty) => {
                let Type::Slice(element) = &**inner else {
                    return;
                };
                let rest_ty = Type::Reference {
                    inner: inner.clone(),
                    mutable: false,
                };
                ((**element).clone(), rest_ty)
            }
            Type::Unknown => (Type::Unknown, Type::Unknown),
            _ => {
                self.record_error(TypeError::PatternTypeMismatch {
                    pattern_ty: "an array".to_string(),
                    scrutinee_ty: scrut_ty.clone(),
                    span,
                });
                (Type::Unknown, Type::Unknown)
            }
        };
        for element in elements {
            self.check_pattern(element, &element_ty, bindings);
        }
        if let Some(ident) = rest.and_then(|r| r.binding.as_ref()) {
            bindings.push((ident.name.clone(), rest_ty, ident.span));
        }
    }

    /// Check an enum-variant pattern and bind its payload sub-patterns.
    fn check_enum_pattern(
        &mut self,
        base: &str,
        variant: &str,
        payload: &EnumPatternPayload,
        scrut_ty: &Type,
        span: Span,
        bindings: &mut Vec<(String, Type, Span)>,
    ) {
        // The scrutinee must be exactly this enum (nominal typing). A pattern written
        // with a generic enum's base name (`Option::Some`) matches the monomorphized
        // instance the scrutinee has, and its payload binds at the instance's concrete
        // types.
        let enum_name = match scrut_ty {
            Type::Unknown => base,
            Type::Enum(name)
                if name == base || self.enum_instance_base(name).as_deref() == Some(base) =>
            {
                name.as_str()
            }
            _ => {
                self.record_error(TypeError::PatternTypeMismatch {
                    pattern_ty: format!("{}::{}", base, variant),
                    scrutinee_ty: scrut_ty.clone(),
                    span,
                });
                return;
            }
        };

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
                    self.check_pattern(sub, field_ty, bindings);
                }
            }
            (VariantForm::Struct, EnumPatternPayload::Struct(field_pats)) => {
                for fp in field_pats {
                    match fields
                        .iter()
                        .find(|(n, _)| n.as_deref() == Some(fp.field.name.as_str()))
                    {
                        Some((_, field_ty)) => self.check_pattern(&fp.pattern, field_ty, bindings),
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

    /// Check that a literal pattern's type is compatible with the scrutinee.
    fn check_literal_pattern(&mut self, lit: &Literal, scrut_ty: &Type, span: Span) {
        if matches!(scrut_ty, Type::Unknown) {
            return;
        }
        let ok = match lit {
            Literal::Integer(_, None) => scrut_ty.is_integer(),
            Literal::Integer(_, Some(suffix)) => &suffix_to_type(suffix) == scrut_ty,
            Literal::Char(_) => scrut_ty.is_char(),
            Literal::Boolean(_) => scrut_ty.is_bool(),
            Literal::Float(_, _) | Literal::String(_) => {
                if scrut_ty.is_float() || matches!(scrut_ty, Type::String) {
                    self.record_error(TypeError::UntestableLiteralPattern {
                        ty: scrut_ty.clone(),
                        span,
                    });
                    return;
                }
                false
            }
        };
        if !ok {
            self.record_error(TypeError::PatternTypeMismatch {
                pattern_ty: literal_type_word(lit).to_string(),
                scrutinee_ty: scrut_ty.clone(),
                span,
            });
        }
    }
}

/// Whether `ty` is `&[T]` or `&mut [T]`.
pub(crate) fn is_slice_borrow(ty: &Type) -> bool {
    matches!(ty, Type::Reference { inner, .. } if matches!(**inner, Type::Slice(_)))
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
