// Enum construction: unit paths, tuple calls, and struct-variant literals.
//
// Reached from the `check_expr` dispatch in this module's `mod.rs`. Every file
// here adds methods to the same `impl TypeChecker` block.

use super::{declarations, mentions_type_parameter, TypeChecker, VariantForm};
use crate::errors::TypeError;
use crate::types::Type;
use ast_types::{Expr, FieldInit};
use shared_types::{Identifier, Span};
use std::collections::HashMap;

impl TypeChecker {
    /// The enum a construction written as `E::V` targets: `E` itself when it is a
    /// plain enum, or the monomorphized instance of a generic `E` that the surrounding
    /// expected type names. A generic `E` with no usable context resolves to the base
    /// name, which callers detect with [`TypeChecker::is_generic_enum`] and either infer
    /// from the payload or reject.
    pub(super) fn enum_construction_target(&self, base: &str, expected: Option<&Type>) -> String {
        self.enum_instance_from_expected(base, expected)
            .unwrap_or_else(|| base.to_string())
    }

    /// Monomorphize a generic enum from the type arguments inferred at a construction
    /// site, falling back to the enclosing return type for any the payload left
    /// undetermined (see [`TypeChecker::enum_return_type_args`]). Records
    /// `GenericEnumNotInferable` and returns `None` when a parameter is determined by
    /// neither.
    pub(super) fn instantiate_inferred_enum(
        &mut self,
        base: &str,
        subst: &HashMap<String, Type>,
        span: Span,
    ) -> Option<Type> {
        let generics = self.generic_enum_params(base);
        let from_return = self.enum_return_type_args(base);
        let mut args = Vec::with_capacity(generics.len());
        for (index, param) in generics.iter().enumerate() {
            let inferred = subst
                .get(param)
                .cloned()
                .or_else(|| from_return.as_ref().and_then(|a| a.get(index).cloned()));
            match inferred {
                Some(ty) => args.push(ty),
                None => {
                    self.record_error(TypeError::GenericEnumNotInferable {
                        name: base.to_string(),
                        span,
                    });
                    return None;
                }
            }
        }
        Some(self.instantiate_generic_enum(base, &args, span))
    }

    /// Type-check a bare path enum construction `E::V`: valid only for a
    /// unit variant. A tuple/struct variant used here is a form error. Returns the
    /// enum type for error recovery in every case.
    pub(super) fn check_enum_unit_path(
        &mut self,
        base: &str,
        variant: &str,
        span: Span,
        expected: Option<&Type>,
    ) -> Type {
        let enum_name = &self.enum_construction_target(base, expected);
        let recovery = Type::Enum(enum_name.clone());
        let Some(info) = self.lookup_enum_variant(enum_name, variant) else {
            self.record_error(TypeError::UnknownEnumVariant {
                enum_name: enum_name.clone(),
                variant: variant.to_string(),
                span,
            });
            return recovery;
        };
        match info.form {
            VariantForm::Unit => {}
            VariantForm::Tuple => self.record_error(TypeError::EnumVariantFormMismatch {
                enum_name: enum_name.to_string(),
                variant: variant.to_string(),
                expected: "tuple".to_string(),
                hint: "construct it with arguments, e.g. `E::V(...)`".to_string(),
                span,
            }),
            VariantForm::Struct => self.record_error(TypeError::EnumVariantFormMismatch {
                enum_name: enum_name.to_string(),
                variant: variant.to_string(),
                expected: "struct".to_string(),
                hint: "construct it with braces, e.g. `E::V { field: ... }`".to_string(),
                span,
            }),
        }
        // A unit variant of a generic enum carries no payload, so its type arguments come
        // entirely from context: the expected type (already applied above) or the
        // enclosing return type.
        if self.is_generic_enum(enum_name) {
            return self
                .instantiate_inferred_enum(enum_name, &HashMap::new(), span)
                .unwrap_or(Type::Unknown);
        }
        recovery
    }

    /// Type-check a tuple-variant enum construction `E::V(args)`: the variant
    /// must be a tuple variant, and the arguments must match its field types by
    /// position. For a generic enum the type arguments come from the expected type
    /// when there is one, else they are inferred by unifying the template's payload
    /// against the argument types. Returns the enum type for error recovery.
    pub(super) fn check_enum_tuple_call(
        &mut self,
        base: &str,
        variant: &str,
        args: &[Expr],
        span: Span,
        expected: Option<&Type>,
    ) -> Type {
        let enum_name = &self.enum_construction_target(base, expected);
        let recovery = Type::Enum(enum_name.clone());
        let info = match self.lookup_enum_variant(enum_name, variant) {
            Some(info) => info,
            None => {
                self.record_error(TypeError::UnknownEnumVariant {
                    enum_name: enum_name.to_string(),
                    variant: variant.to_string(),
                    span,
                });
                for arg in args {
                    let _ = self.check_expr(arg, None);
                }
                return recovery;
            }
        };

        match info.form {
            VariantForm::Tuple => {}
            VariantForm::Unit => {
                self.record_error(TypeError::EnumVariantFormMismatch {
                    enum_name: enum_name.to_string(),
                    variant: variant.to_string(),
                    expected: "unit".to_string(),
                    hint: "construct it without arguments, e.g. `E::V`".to_string(),
                    span,
                });
                for arg in args {
                    let _ = self.check_expr(arg, None);
                }
                return recovery;
            }
            VariantForm::Struct => {
                self.record_error(TypeError::EnumVariantFormMismatch {
                    enum_name: enum_name.to_string(),
                    variant: variant.to_string(),
                    expected: "struct".to_string(),
                    hint: "construct it with braces, e.g. `E::V { field: ... }`".to_string(),
                    span,
                });
                for arg in args {
                    let _ = self.check_expr(arg, None);
                }
                return recovery;
            }
        }

        // Clone the field types so the immutable enum-table borrow ends before the
        // mutable `check_expr` calls below.
        let field_tys: Vec<Type> = info.fields.iter().map(|(_, t)| t.clone()).collect();

        if args.len() != field_tys.len() {
            self.record_error(TypeError::EnumVariantArityMismatch {
                enum_name: enum_name.to_string(),
                variant: variant.to_string(),
                expected: field_tys.len(),
                found: args.len(),
                span,
            });
        }

        // An unresolved generic base still carries type-parameter placeholders in its
        // payload: check each argument without imposing a placeholder as its context,
        // then unify to recover the type arguments.
        let inferring = self.is_generic_enum(enum_name);
        let mut subst: HashMap<String, Type> = HashMap::new();
        let mut arg_tys: Vec<Option<Type>> = Vec::with_capacity(args.len());
        for (arg, declared) in args.iter().zip(field_tys.iter()) {
            let ctx = (!mentions_type_parameter(declared)).then(|| declared.clone());
            let arg_ty = self.check_expr(arg, ctx.as_ref());
            if inferring {
                if let Some(ty) = &arg_ty {
                    declarations::unify_generic(declared, ty, &mut subst);
                }
            }
            arg_tys.push(arg_ty);
        }

        let (result, payload_tys) = if inferring {
            let Some(instance) = self.instantiate_inferred_enum(enum_name, &subst, span) else {
                return Type::Unknown;
            };
            let concrete = match &instance {
                Type::Enum(name) => self
                    .lookup_enum_variant(name, variant)
                    .map(|info| info.fields.iter().map(|(_, t)| t.clone()).collect())
                    .unwrap_or_default(),
                _ => Vec::new(),
            };
            (instance, concrete)
        } else {
            (recovery, field_tys)
        };

        for ((arg, declared), arg_ty) in args.iter().zip(payload_tys.iter()).zip(arg_tys.iter()) {
            let Some(arg_ty) = arg_ty else { continue };
            if !arg_ty.is_compatible_with(declared) {
                self.record_error(TypeError::Mismatch {
                    expected: declared.clone(),
                    found: arg_ty.clone(),
                    span: arg.span(),
                });
            }
        }
        result
    }

    /// Type-check a struct-variant enum construction `E::V { field: expr, ... }`
    /// Every declared field must be provided exactly once with a matching
    /// type, and no unknown fields. Returns the enum type for error recovery.
    pub(super) fn check_enum_struct_literal(
        &mut self,
        base: &Identifier,
        variant: &Identifier,
        fields: &[FieldInit],
        span: Span,
        expected: Option<&Type>,
    ) -> Type {
        let enum_name = self.enum_construction_target(&base.name, expected);
        let recovery = Type::Enum(enum_name.clone());

        if !self.enum_defs.contains_key(&enum_name) {
            self.record_error(TypeError::UnknownPathType {
                type_name: enum_name,
                member: variant.name.clone(),
                span,
            });
            for field in fields {
                let _ = self.check_expr(&field.value, None);
            }
            return recovery;
        }

        let info_fields: Vec<(Option<String>, Type)> =
            match self.lookup_enum_variant(&enum_name, &variant.name) {
                Some(info) if info.form == VariantForm::Struct => info.fields.clone(),
                Some(_) => {
                    self.record_error(TypeError::EnumVariantFormMismatch {
                        enum_name,
                        variant: variant.name.clone(),
                        expected: "non-struct".to_string(),
                        hint: "this variant is not constructed with braces".to_string(),
                        span,
                    });
                    for field in fields {
                        let _ = self.check_expr(&field.value, None);
                    }
                    return recovery;
                }
                None => {
                    self.record_error(TypeError::UnknownEnumVariant {
                        enum_name,
                        variant: variant.name.clone(),
                        span,
                    });
                    for field in fields {
                        let _ = self.check_expr(&field.value, None);
                    }
                    return recovery;
                }
            };

        // An unresolved generic base carries placeholders in its payload; the field
        // values determine the type arguments.
        let inferring = self.is_generic_enum(&enum_name);
        let mut subst: HashMap<String, Type> = HashMap::new();
        let mut seen: HashMap<String, Span> = HashMap::new();
        let mut provided: Vec<(&Expr, Type, Type)> = Vec::new();
        for FieldInit {
            name: fname,
            value,
            span: fspan,
        } in fields
        {
            if seen.insert(fname.name.clone(), *fspan).is_some() {
                self.record_error(TypeError::DuplicateEnumField {
                    enum_name: enum_name.clone(),
                    variant: variant.name.clone(),
                    field: fname.name.clone(),
                    span: *fspan,
                });
                continue;
            }

            match info_fields
                .iter()
                .find(|(n, _)| n.as_deref() == Some(&fname.name))
                .map(|(_, t)| t.clone())
            {
                Some(declared) => {
                    let ctx = (!mentions_type_parameter(&declared)).then(|| declared.clone());
                    let actual = self.check_expr(value, ctx.as_ref());
                    if let Some(actual) = actual {
                        if inferring {
                            declarations::unify_generic(&declared, &actual, &mut subst);
                        }
                        provided.push((value, declared, actual));
                    }
                }
                None => {
                    self.record_error(TypeError::UnknownEnumField {
                        enum_name: enum_name.clone(),
                        variant: variant.name.clone(),
                        field: fname.name.clone(),
                        span: *fspan,
                    });
                    let _ = self.check_expr(value, None);
                }
            }
        }

        for (field_name, _) in &info_fields {
            if let Some(field_name) = field_name {
                if !seen.contains_key(field_name) {
                    self.record_error(TypeError::MissingEnumField {
                        enum_name: enum_name.clone(),
                        variant: variant.name.clone(),
                        field: field_name.clone(),
                        span,
                    });
                }
            }
        }

        let result = if inferring {
            match self.instantiate_inferred_enum(&enum_name, &subst, span) {
                Some(instance) => instance,
                None => return Type::Unknown,
            }
        } else {
            recovery
        };

        for (value, declared, actual) in &provided {
            // Under inference the declared type is a placeholder the argument bound, so
            // the concrete comparison is the substituted one.
            let declared = declarations::substitute_generic(declared, &subst);
            if !actual.is_compatible_with(&declared) {
                self.record_error(TypeError::Mismatch {
                    expected: declared,
                    found: actual.clone(),
                    span: value.span(),
                });
            }
        }
        result
    }
}
