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
    /// Payload types are restricted to scalar `Copy` primitives in this phase
    /// (integers, floats, `bool`, `char`); a non-scalar payload (string, struct,
    /// array, tuple, reference) is rejected with `UnsupportedEnumPayload` so the
    /// tagged-union codegen stays a fixed-width slot layout. Broader payloads land
    /// with pattern matching and heap support.
    pub(crate) fn register_enum(&mut self, def: &EnumDef) {
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

        let variants = self.resolve_variants(def);
        self.enum_defs.insert(def.name.name.clone(), variants);
    }

    /// Resolve an enum-variant payload type, rejecting any non-scalar payload with
    /// `UnsupportedEnumPayload` and recovering as `Type::Unknown`.
    pub(super) fn resolve_enum_payload_type(&mut self, ty: &ast_types::Type) -> Type {
        let Some(resolved) = self.resolve_type(ty) else {
            return Type::Unknown;
        };
        // A type-parameter placeholder inside a generic template carries no scalar
        // decision yet; the check runs again per instance against the concrete argument.
        if matches!(resolved, Type::Generic(_)) || Self::is_scalar_payload(&resolved) {
            resolved
        } else {
            self.record_error(TypeError::UnsupportedEnumPayload {
                ty: resolved,
                span: ty.span(),
            });
            Type::Unknown
        }
    }

    /// Register a generic enum template.
    ///
    /// Like a generic struct, a generic enum is not itself a usable type — each
    /// distinct set of type arguments is monomorphized into a distinct nominal enum on
    /// demand. The template's variants (carrying [`Type::Generic`] placeholders) are kept
    /// in `enum_defs` under the base name so a construction site can infer the type
    /// arguments by unifying its payload against them.
    pub(crate) fn register_generic_enum(&mut self, def: &EnumDef) {
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

        self.enter_generic_scope(&def.generics, &[]);
        let variants = self.resolve_variants(def);
        self.exit_generic_scope();

        self.enum_defs.insert(def.name.name.clone(), variants);
        self.generic_enums
            .insert(def.name.name.clone(), def.clone());
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
    /// must be a scalar `Copy` primitive — the same restriction a non-generic enum's
    /// payload carries, so `Option<i32>` is available while `Option<string>` is not yet.
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
                _ if !self.is_type_copy(arg) => {
                    self.record_error(TypeError::GenericArgumentNotCopy {
                        param: gp.name.name.clone(),
                        ty: arg.clone(),
                        span,
                    })
                }
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
                    if Self::is_scalar_payload(&concrete) || matches!(concrete, Type::Unknown) {
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
    pub(super) fn is_scalar_payload(ty: &Type) -> bool {
        ty.is_integer()
            || ty.is_float()
            || ty.is_half_float()
            || matches!(ty, Type::Bool | Type::Char)
    }
}
