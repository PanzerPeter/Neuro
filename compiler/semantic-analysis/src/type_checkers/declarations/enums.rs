//! Enum declarations: variant registration, payload typing, and generic
//! enum instantiation.
//!
//! One of the declaration-kind modules under `declarations`; each adds methods
//! to the same `impl TypeChecker` block.

use super::{mangle_struct_instance, substitute_generic};
use crate::errors::TypeError;
use crate::type_checkers::{EnumVariantInfo, TypeChecker, VariantForm};
use crate::types::Type;
use ast_types::{EnumDef, VariantPayload};
use shared_types::Span;
use std::collections::HashMap;

impl TypeChecker {
    /// Register an enum definition: its variants, their construction form, and each
    /// payload field's resolved type.
    ///
    /// A payload field may hold any sized type, `Copy` or not: the tagged-union
    /// layout sizes its slots to the widest payload field rather than to a word.
    /// An unsized type (`dyn Trait`, `[T]`) is rejected with `UnsupportedEnumPayload`,
    /// since a slot has to have a width.
    pub(crate) fn predeclare_enum(&mut self, def: &EnumDef) {
        if self.enum_defs.contains_key(&def.name.name)
            || self.struct_defs.contains_key(&def.name.name)
            || self.generic_enums.contains_key(&def.name.name)
        {
            self.record_error(TypeError::EnumAlreadyDefined {
                name: def.name.name.clone(),
                span: def.name.span,
            });
            return;
        }
        // The name alone, so a payload naming a struct (and a struct field naming this
        // enum) both resolve regardless of source order. Variants land in
        // `resolve_enum_variants`, once every nominal name is known.
        self.enum_defs.insert(def.name.name.clone(), Vec::new());
        if !def.generics.is_empty() {
            self.generic_enums
                .insert(def.name.name.clone(), def.clone());
        }
    }

    /// Resolve a predeclared enum's variant payloads, now that every nominal name is
    /// registered. A payload may be any sized type, including a struct declared after
    /// the enum.
    pub(crate) fn resolve_enum_variants(&mut self, def: &EnumDef) {
        if !self
            .enum_defs
            .get(&def.name.name)
            .is_some_and(|variants| variants.is_empty())
        {
            return;
        }
        let variants = if def.generics.is_empty() {
            self.resolve_variants(def)
        } else {
            self.enter_generic_scope(&def.generics, &[]);
            let variants = self.resolve_variants(def);
            self.exit_generic_scope();
            variants
        };
        self.enum_defs.insert(def.name.name.clone(), variants);
    }

    /// Resolve an enum-variant payload type, rejecting an unsized one with
    /// `UnsupportedEnumPayload` and recovering as `Type::Unknown`.
    pub(super) fn resolve_enum_payload_type(&mut self, ty: &ast_types::Type) -> Type {
        let Some(resolved) = self.resolve_type(ty) else {
            return Type::Unknown;
        };
        // A type-parameter placeholder inside a generic template stands for whatever an
        // instance substitutes; the check runs again per instance on the concrete type.
        if matches!(resolved, Type::Generic(_)) || Self::is_sized_payload(&resolved) {
            resolved
        } else {
            self.record_error(TypeError::UnsupportedEnumPayload {
                ty: resolved,
                span: ty.span(),
            });
            Type::Unknown
        }
    }

    /// Resolve every variant of an enum definition into its checked form.
    pub(super) fn resolve_variants(&mut self, def: &EnumDef) -> Vec<EnumVariantInfo> {
        let mut variants: Vec<EnumVariantInfo> = Vec::with_capacity(def.variants.len());
        for variant in &def.variants {
            let (form, fields) = match &variant.payload {
                VariantPayload::Unit => (VariantForm::Unit, Vec::new()),
                VariantPayload::Tuple(tys) => {
                    let mut fields = Vec::with_capacity(tys.len());
                    for ty in tys {
                        let resolved = self.resolve_enum_payload_type(ty);
                        fields.push((None, resolved));
                    }
                    (VariantForm::Tuple, fields)
                }
                VariantPayload::Struct(field_defs) => {
                    let mut fields = Vec::with_capacity(field_defs.len());
                    for field in field_defs {
                        let resolved = self.resolve_enum_payload_type(&field.ty);
                        fields.push((Some(field.name.name.clone()), resolved));
                    }
                    (VariantForm::Struct, fields)
                }
            };
            variants.push(EnumVariantInfo {
                name: variant.name.name.clone(),
                form,
                fields,
            });
        }
        variants
    }

    /// Materialize a monomorphized instance of a generic enum with concrete type
    /// arguments and return its distinct nominal [`Type::Enum`]. Idempotent per instance.
    ///
    /// Each payload type is the template's type with the arguments substituted in, and
    /// carries the same sizedness requirement a non-generic enum's payload does.
    pub(crate) fn instantiate_generic_enum(
        &mut self,
        base: &str,
        args: &[Type],
        span: Span,
    ) -> Type {
        let template = match self.generic_enums.get(base) {
            Some(t) => t.clone(),
            None => {
                self.record_error(TypeError::NotAGenericType {
                    name: base.to_string(),
                    span,
                });
                return Type::Unknown;
            }
        };
        if args.len() != template.generics.len() {
            self.record_error(TypeError::GenericArgCountMismatch {
                name: base.to_string(),
                expected: template.generics.len(),
                found: args.len(),
                span,
            });
            return Type::Unknown;
        }

        // A struct field may name an instance (`Option<i32>`) while the template's own
        // payloads are still unresolved, because a payload may in turn name a struct.
        // Resolving the template here is idempotent and breaks that cycle.
        self.resolve_enum_variants(&template);

        let mangled = mangle_struct_instance(base, args);
        if self.enum_defs.contains_key(&mangled) {
            return Type::Enum(mangled);
        }

        let mut subst: HashMap<String, Type> = HashMap::new();
        for (gp, arg) in template.generics.iter().zip(args.iter()) {
            let is_const = matches!(gp.kind, ast_types::GenericParamKind::Const(_));
            match arg {
                Type::ConstValue(_) if is_const => {}
                Type::ConstValue(_) => self.record_error(TypeError::TurbofishKindMismatch {
                    param: gp.name.name.clone(),
                    expected: "type".to_string(),
                    span,
                }),
                _ if is_const => self.record_error(TypeError::TurbofishKindMismatch {
                    param: gp.name.name.clone(),
                    expected: "const".to_string(),
                    span,
                }),
                _ => {}
            }
            subst.insert(gp.name.name.clone(), arg.clone());
        }

        let template_variants = self.enum_defs.get(base).cloned().unwrap_or_default();
        let mut variants: Vec<EnumVariantInfo> = Vec::with_capacity(template_variants.len());
        for variant in &template_variants {
            let mut fields = Vec::with_capacity(variant.fields.len());
            for (name, ty) in &variant.fields {
                let concrete = substitute_generic(ty, &subst);
                let concrete =
                    if Self::is_sized_payload(&concrete) || matches!(concrete, Type::Unknown) {
                        concrete
                    } else {
                        self.record_error(TypeError::UnsupportedEnumPayload { ty: concrete, span });
                        Type::Unknown
                    };
                fields.push((name.clone(), concrete));
            }
            variants.push(EnumVariantInfo {
                name: variant.name.clone(),
                form: variant.form,
                fields,
            });
        }

        self.enum_defs.insert(mangled.clone(), variants);
        self.enum_instances
            .insert(mangled.clone(), (base.to_string(), args.to_vec()));
        Type::Enum(mangled)
    }

    /// Whether `ty` is a scalar `Copy` primitive admissible as an enum payload in
    /// this phase: any integer, full- or half-precision float, `bool`, or `char`.
    pub(super) fn is_sized_payload(ty: &Type) -> bool {
        !matches!(ty, Type::Void | Type::Slice(_) | Type::DynObject(_))
    }
}
