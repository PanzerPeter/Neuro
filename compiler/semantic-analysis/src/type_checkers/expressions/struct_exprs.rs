// Struct literals, newtype construction, and field reads.
//
// Reached from the `check_expr` dispatch in this module's `mod.rs`. Every file
// here adds methods to the same `impl TypeChecker` block.

use super::{declarations, mentions_type_parameter, TypeChecker};
use crate::errors::TypeError;
use crate::types::Type;
use ast_types::{Expr, FieldInit};
use shared_types::{Identifier, Span};
use std::collections::HashMap;

impl TypeChecker {
    /// Type-check a newtype construction `Name(value)`: exactly one argument,
    /// whose type must match the newtype's inner type. Yields the newtype.
    pub(super) fn check_newtype_construction(
        &mut self,
        name: &str,
        inner: &Type,
        args: &[ast_types::Expr],
        span: shared_types::Span,
    ) -> Type {
        if args.len() != 1 {
            self.record_error(TypeError::ArgumentCountMismatch {
                expected: 1,
                found: args.len(),
                span,
            });
            // Still type-check any arguments so their own errors surface.
            for arg in args {
                let _ = self.check_expr(arg, Some(inner));
            }
            return Type::Newtype(name.to_string());
        }

        if let Some(arg_ty) = self.check_expr(&args[0], Some(inner)) {
            if !arg_ty.is_compatible_with(inner) {
                self.record_error(TypeError::Mismatch {
                    expected: inner.clone(),
                    found: arg_ty,
                    span: args[0].span(),
                });
            }
        }
        Type::Newtype(name.to_string())
    }

    /// Type-check a generic struct literal: infer each type parameter by
    /// unifying the template's field types against the provided field values, then
    /// monomorphize into a concrete instance. Type arguments are Copy-restricted
    /// (enforced by [`Self::instantiate_generic_struct`]).
    pub(super) fn check_generic_struct_literal(
        &mut self,
        name: &shared_types::Identifier,
        fields: &[ast_types::FieldInit],
        base: &Option<Box<ast_types::Expr>>,
        span: shared_types::Span,
    ) -> Type {
        let template_fields = self
            .struct_defs
            .get(&name.name)
            .cloned()
            .unwrap_or_default();
        let generics: Vec<String> = self
            .generic_structs
            .get(&name.name)
            .map(|d| d.generics.iter().map(|g| g.name.name.clone()).collect())
            .unwrap_or_default();

        let mut subst: HashMap<String, Type> = HashMap::new();
        let mut seen: HashMap<String, Span> = HashMap::new();
        for ast_types::FieldInit {
            name: fname,
            value,
            span: fspan,
        } in fields
        {
            if seen.insert(fname.name.clone(), *fspan).is_some() {
                self.record_error(TypeError::DuplicateStructField {
                    field_name: fname.name.clone(),
                    span: *fspan,
                });
                continue;
            }
            match template_fields.iter().find(|(n, _)| n == &fname.name) {
                Some((_, expected)) => {
                    self.reject_private_field(&name.name, &fname.name, *fspan);
                    let expected = expected.clone();
                    // A field whose type is fully concrete (mentions no type/const
                    // parameter) gives the value its contextual type so a bare literal
                    // infers correctly; a parameterized field is checked with no
                    // expectation so it drives inference instead.
                    let expected_ctx = if mentions_type_parameter(&expected) {
                        None
                    } else {
                        Some(&expected)
                    };
                    let actual = self
                        .check_expr(value, expected_ctx)
                        .unwrap_or(Type::Unknown);
                    if !matches!(actual, Type::Unknown)
                        && !declarations::unify_generic(&expected, &actual, &mut subst)
                    {
                        self.record_error(TypeError::Mismatch {
                            expected: declarations::substitute_generic(&expected, &subst),
                            found: actual,
                            span: value.span(),
                        });
                    }
                    self.record_move(value);
                }
                None => {
                    self.record_error(TypeError::UnknownField {
                        struct_name: name.name.clone(),
                        field_name: fname.name.clone(),
                        span: *fspan,
                    });
                    let _ = self.check_expr(value, None);
                }
            }
        }

        // Without a `..base` source every field must be provided.
        if base.is_none() {
            for (field_name, _) in &template_fields {
                if !seen.contains_key(field_name) {
                    self.record_error(TypeError::MissingStructField {
                        struct_name: name.name.clone(),
                        field_name: field_name.clone(),
                        span,
                    });
                }
            }
        }

        // Every type parameter must have been inferred from a field value.
        let mut args = Vec::with_capacity(generics.len());
        for g in &generics {
            match subst.get(g) {
                Some(t) => args.push(t.clone()),
                None => return Type::Unknown,
            }
        }

        let inst = self.instantiate_generic_struct(&name.name, &args, span);

        // A `..base` source, when present, must be the same monomorphized instance.
        if let Some(base_expr) = base {
            self.reject_private_update(&name.name, &seen, span);
            if let Some(base_ty) = self.check_expr(base_expr, Some(&inst)) {
                if !base_ty.is_compatible_with(&inst) {
                    self.record_error(TypeError::Mismatch {
                        expected: inst.clone(),
                        found: base_ty,
                        span: base_expr.span(),
                    });
                }
            }
        }

        inst
    }

    pub(super) fn check_struct_literal_expr(
        &mut self,
        name: &Identifier,
        fields: &[FieldInit],
        base: &Option<Box<Expr>>,
        span: &Span,
    ) -> Option<Type> {
        // A generic struct literal `Pair { first: 1, second: 2.0 }` infers its
        // type arguments from the field values and monomorphizes.
        if self.is_generic_struct(&name.name) {
            return Some(self.check_generic_struct_literal(name, fields, base, *span));
        }

        let def = if let Some(d) = self.struct_defs.get(&name.name).cloned() {
            d
        } else {
            self.record_error(TypeError::UnknownStruct {
                name: name.name.clone(),
                span: name.span,
            });
            return None;
        };

        // Track which fields have been provided to detect duplicates and missing fields
        let mut seen: HashMap<String, Span> = HashMap::new();
        for FieldInit {
            name: fname,
            value,
            span: fspan,
        } in fields
        {
            if let Some(prev_span) = seen.insert(fname.name.clone(), *fspan) {
                let _ = prev_span;
                self.record_error(TypeError::DuplicateStructField {
                    field_name: fname.name.clone(),
                    span: *fspan,
                });
                continue;
            }

            let expected_field_ty = def
                .iter()
                .find(|(n, _)| n == &fname.name)
                .map(|(_, t)| t.clone());

            if let Some(ref expected_ty) = expected_field_ty {
                self.reject_private_field(&name.name, &fname.name, *fspan);
                if let Some(actual_ty) = self.check_expr(value, Some(expected_ty)) {
                    if !actual_ty.is_compatible_with(expected_ty) {
                        self.record_error(TypeError::Mismatch {
                            expected: expected_ty.clone(),
                            found: actual_ty,
                            span: value.span(),
                        });
                    }
                }
            } else {
                self.record_error(TypeError::UnknownField {
                    struct_name: name.name.clone(),
                    field_name: fname.name.clone(),
                    span: *fspan,
                });
                // Still check the value expression for cascaded errors
                let _ = self.check_expr(value, None);
            }
        }

        // A `..base` source supplies every unlisted field, so missing
        // fields are only an error for a plain literal. The base itself
        // must be the same struct type.
        if let Some(base_expr) = base {
            self.reject_private_update(&name.name, &seen, *span);
            let expected = Type::Struct(name.name.clone());
            if let Some(base_ty) = self.check_expr(base_expr, Some(&expected)) {
                if !base_ty.is_compatible_with(&expected) {
                    self.record_error(TypeError::Mismatch {
                        expected,
                        found: base_ty,
                        span: base_expr.span(),
                    });
                }
            }
        } else {
            for (field_name, _) in &def {
                if !seen.contains_key(field_name) {
                    self.record_error(TypeError::MissingStructField {
                        struct_name: name.name.clone(),
                        field_name: field_name.clone(),
                        span: *span,
                    });
                }
            }
        }

        Some(Type::Struct(name.name.clone()))
    }

    pub(super) fn check_field_access_expr(
        &mut self,
        object: &Expr,
        field: &Identifier,
        span: &Span,
    ) -> Option<Type> {
        let obj_ty = self.check_expr(object, None).unwrap_or(Type::Unknown);
        if matches!(obj_ty, Type::Unknown) {
            return Some(Type::Unknown);
        }

        // Auto-deref through an immutable borrow: `r.field` reads a field of the
        // referent when `r: &Struct`.
        let struct_name = match obj_ty.referent() {
            Type::Struct(n) => n.clone(),
            other => {
                self.record_error(TypeError::UnknownField {
                    struct_name: other.to_string(),
                    field_name: field.name.clone(),
                    span: *span,
                });
                return Some(Type::Unknown);
            }
        };

        let def = self.struct_defs.get(&struct_name).cloned();
        if let Some(def) = def {
            if let Some((_, field_ty)) = def.iter().find(|(n, _)| n == &field.name) {
                self.reject_private_field(&struct_name, &field.name, field.span);
                Some(field_ty.clone())
            } else {
                self.record_error(TypeError::UnknownField {
                    struct_name,
                    field_name: field.name.clone(),
                    span: field.span,
                });
                Some(Type::Unknown)
            }
        } else {
            self.record_error(TypeError::UnknownStruct {
                name: struct_name,
                span: *span,
            });
            Some(Type::Unknown)
        }
    }
}
