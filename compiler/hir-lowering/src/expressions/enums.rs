//! Enum construction: variant lookup, payload typing, and the four literal shapes.
//!
//! Reached from the `lower_expr_uncoerced` dispatch in this module's `mod.rs`.
//! Every file here adds methods to the same `impl Lowerer` block.

use ast_types::{Expr, FieldInit};
use neuro_hir::{HirExpr, HirExprKind, HirType};

use super::PayloadFields;
use crate::{Lowerer, LoweringError};

impl Lowerer {
    /// Look up an enum variant by name, returning its discriminant tag (declaration
    /// index) and a clone of its ordered payload fields. The clone frees the enum
    /// table's immutable borrow before the mutable argument lowering that follows.
    pub(crate) fn enum_variant(
        &self,
        enum_name: &str,
        variant: &str,
    ) -> Result<(u32, PayloadFields), LoweringError> {
        let variants = self
            .enums
            .get(enum_name)
            .ok_or_else(|| LoweringError::UnresolvedType {
                name: enum_name.to_string(),
            })?;
        variants
            .iter()
            .enumerate()
            .find(|(_, v)| v.name == variant)
            .map(|(i, v)| (i as u32, v.fields.clone()))
            .ok_or_else(|| LoweringError::UnresolvedCall {
                target: format!("{}::{}", enum_name, variant),
            })
    }

    /// Assemble the single [`HirExprKind::EnumConstruct`] node every surface form
    /// lowers to. `payload` is already in the variant's declared field order.
    pub(super) fn build_enum_construct(
        &self,
        enum_name: &str,
        variant: &str,
        tag: u32,
        payload: Vec<HirExpr>,
        span: shared_types::Span,
    ) -> HirExpr {
        HirExpr::new(
            HirExprKind::EnumConstruct {
                enum_name: enum_name.to_string(),
                variant: variant.to_string(),
                tag,
                payload,
            },
            HirType::Enum(enum_name.to_string()),
            span,
        )
    }

    /// The enum a construction written `E::V` targets: `E` itself for a plain enum, or
    /// the monomorphized instance of a generic `E` the expected type names. A generic
    /// enum with no usable context yields its base name, and the caller infers the type
    /// arguments from the payload.
    pub(super) fn enum_construction_target(
        &self,
        base: &str,
        expected: Option<&HirType>,
    ) -> String {
        self.enum_instance_from_expected(base, expected)
            .unwrap_or_else(|| base.to_string())
    }

    /// The template payload types of one variant of a generic enum, as written (so they
    /// still mention the type parameters), keyed by optional field name.
    pub(super) fn generic_variant_payload(
        &self,
        base: &str,
        variant: &str,
    ) -> Result<Vec<(Option<String>, ast_types::Type)>, LoweringError> {
        let template =
            self.generic_enums
                .get(base)
                .ok_or_else(|| LoweringError::UnresolvedType {
                    name: base.to_string(),
                })?;
        let variant = template
            .variants
            .iter()
            .find(|v| v.name.name == variant)
            .ok_or_else(|| LoweringError::UnresolvedCall {
                target: format!("{}::{}", base, variant),
            })?;
        Ok(match &variant.payload {
            ast_types::VariantPayload::Unit => Vec::new(),
            ast_types::VariantPayload::Tuple(tys) => {
                tys.iter().map(|ty| (None, ty.clone())).collect()
            }
            ast_types::VariantPayload::Struct(fields) => fields
                .iter()
                .map(|f| (Some(f.name.name.clone()), f.ty.clone()))
                .collect(),
        })
    }

    /// Monomorphize a generic enum from the type arguments a construction site's payload
    /// determined, returning the mangled instance name. Arguments the payload leaves
    /// undetermined come from the enclosing return type when it is an instance of the
    /// same enum — the same fallback the checker applies, so both slices agree on which
    /// instance a tail-position `Result::Err(1)` builds.
    pub(super) fn instantiate_inferred_enum(
        &mut self,
        base: &str,
        subst: &std::collections::HashMap<String, HirType>,
        const_subst: &std::collections::HashMap<String, u64>,
    ) -> Result<String, LoweringError> {
        let generics = self
            .generic_enums
            .get(base)
            .map(|def| def.generics.clone())
            .ok_or_else(|| LoweringError::UnresolvedType {
                name: base.to_string(),
            })?;
        let from_return = self.enum_return_type_args(base);
        let mut args = Vec::with_capacity(generics.len());
        for (index, gp) in generics.iter().enumerate() {
            let fallback = from_return.as_ref().and_then(|a| a.get(index).cloned());
            let arg = match &gp.kind {
                ast_types::GenericParamKind::Const(_) => const_subst
                    .get(&gp.name.name)
                    .map(|v| crate::MonoArg::Const(*v))
                    .or(fallback),
                ast_types::GenericParamKind::Type => subst
                    .get(&gp.name.name)
                    .cloned()
                    .map(crate::MonoArg::Type)
                    .or(fallback),
            };
            args.push(arg.ok_or_else(|| LoweringError::UnresolvedType {
                name: format!("generic parameter '{}' of enum '{}'", gp.name.name, base),
            })?);
        }
        self.instantiate_generic_enum(base, &args)
    }

    /// The type arguments of the enclosing function's return type, when it is an
    /// instance of the generic enum `base`.
    pub(super) fn enum_return_type_args(&self, base: &str) -> Option<Vec<crate::MonoArg>> {
        let HirType::Enum(name) = &self.current_return else {
            return None;
        };
        (self.enum_instance_base.get(name).map(|s| s.as_str()) == Some(base))
            .then(|| self.enum_instance_args.get(name).cloned())
            .flatten()
    }

    /// The type-parameter and const-parameter names of a generic enum template.
    pub(super) fn generic_enum_param_names(
        &self,
        base: &str,
    ) -> (
        std::collections::HashSet<String>,
        std::collections::HashSet<String>,
    ) {
        let Some(template) = self.generic_enums.get(base) else {
            return Default::default();
        };
        let types = template
            .generics
            .iter()
            .filter(|g| matches!(g.kind, ast_types::GenericParamKind::Type))
            .map(|g| g.name.name.clone())
            .collect();
        let consts = template
            .generics
            .iter()
            .filter(|g| matches!(g.kind, ast_types::GenericParamKind::Const(_)))
            .map(|g| g.name.name.clone())
            .collect();
        (types, consts)
    }

    /// Lower a unit-variant construction `E::V` — an empty payload. A generic enum's
    /// instance can only come from the expected type here (a unit variant carries
    /// nothing to infer from), which the checker has already enforced.
    pub(super) fn lower_enum_construct(
        &mut self,
        base: &str,
        variant: &str,
        expected: Option<&HirType>,
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        let mut enum_name = self.enum_construction_target(base, expected);
        if self.is_generic_enum(&enum_name) {
            enum_name = self.instantiate_inferred_enum(
                base,
                &std::collections::HashMap::new(),
                &std::collections::HashMap::new(),
            )?;
        }
        let (tag, _) = self.enum_variant(&enum_name, variant)?;
        Ok(self.build_enum_construct(&enum_name, variant, tag, Vec::new(), span))
    }

    /// Lower a tuple-variant construction `E::V(args)`: arguments are positional, so
    /// they are the payload as-is, lowered against the declared field types. For a
    /// generic enum with no expected instance, the argument types determine the type
    /// arguments and the instance is monomorphized here.
    pub(super) fn lower_enum_tuple_call(
        &mut self,
        base: &str,
        variant: &str,
        args: &[Expr],
        expected: Option<&HirType>,
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        let mut enum_name = self.enum_construction_target(base, expected);
        if self.is_generic_enum(&enum_name) {
            let template = self.generic_variant_payload(base, variant)?;
            let (gnames, cnames) = self.generic_enum_param_names(base);
            let mut subst = std::collections::HashMap::new();
            let mut const_subst = std::collections::HashMap::new();
            let mut payload = Vec::with_capacity(args.len());
            for (arg, (_, declared)) in args.iter().zip(template.iter()) {
                let lowered = self.lower_expr(arg, None)?;
                crate::unify_ast_hir(
                    declared,
                    &lowered.ty,
                    &gnames,
                    &cnames,
                    &mut subst,
                    &mut const_subst,
                );
                payload.push(lowered);
            }
            enum_name = self.instantiate_inferred_enum(base, &subst, &const_subst)?;
            let (tag, _) = self.enum_variant(&enum_name, variant)?;
            return Ok(self.build_enum_construct(&enum_name, variant, tag, payload, span));
        }
        let (tag, fields) = self.enum_variant(&enum_name, variant)?;
        let field_tys: Vec<HirType> = fields.into_iter().map(|(_, t)| t).collect();
        let payload = self.lower_args(args, &field_tys)?;
        Ok(self.build_enum_construct(&enum_name, variant, tag, payload, span))
    }

    /// Lower a struct-variant construction `E::V { field: expr, ... }`: the provided
    /// fields are reordered into the variant's declared field order before becoming
    /// the payload, so codegen sees a single positional layout.
    pub(super) fn lower_enum_struct_literal(
        &mut self,
        base: &str,
        variant: &str,
        fields: &[FieldInit],
        expected: Option<&HirType>,
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        let enum_name = self.enum_construction_target(base, expected);
        if self.is_generic_enum(&enum_name) {
            return self.lower_generic_enum_struct_literal(base, variant, fields, span);
        }
        let (tag, declared) = self.enum_variant(&enum_name, variant)?;
        let mut payload = Vec::with_capacity(declared.len());
        for (field_name, field_ty) in &declared {
            let Some(field_name) = field_name else {
                return Err(LoweringError::Malformed {
                    detail: format!(
                        "tuple variant '{}::{}' constructed with field names",
                        enum_name, variant
                    ),
                });
            };
            let provided = fields
                .iter()
                .find(|f| &f.name.name == field_name)
                .ok_or_else(|| LoweringError::Malformed {
                    detail: format!(
                        "missing field '{}' for enum variant '{}::{}'",
                        field_name, enum_name, variant
                    ),
                })?;
            payload.push(self.lower_expr(&provided.value, Some(field_ty))?);
        }
        Ok(self.build_enum_construct(&enum_name, variant, tag, payload, span))
    }

    /// Lower a struct-variant construction of a generic enum whose instance is not given
    /// by context: the field values determine the type arguments, then the payload is
    /// reordered into the variant's declared field order.
    pub(super) fn lower_generic_enum_struct_literal(
        &mut self,
        base: &str,
        variant: &str,
        fields: &[FieldInit],
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        let template = self.generic_variant_payload(base, variant)?;
        let (gnames, cnames) = self.generic_enum_param_names(base);
        let mut subst = std::collections::HashMap::new();
        let mut const_subst = std::collections::HashMap::new();
        let mut payload = Vec::with_capacity(template.len());
        for (field_name, declared) in &template {
            let Some(field_name) = field_name else {
                return Err(LoweringError::Malformed {
                    detail: format!(
                        "tuple variant '{}::{}' constructed with field names",
                        base, variant
                    ),
                });
            };
            let provided = fields
                .iter()
                .find(|f| &f.name.name == field_name)
                .ok_or_else(|| LoweringError::Malformed {
                    detail: format!(
                        "missing field '{}' for enum variant '{}::{}'",
                        field_name, base, variant
                    ),
                })?;
            let lowered = self.lower_expr(&provided.value, None)?;
            crate::unify_ast_hir(
                declared,
                &lowered.ty,
                &gnames,
                &cnames,
                &mut subst,
                &mut const_subst,
            );
            payload.push(lowered);
        }
        let enum_name = self.instantiate_inferred_enum(base, &subst, &const_subst)?;
        let (tag, _) = self.enum_variant(&enum_name, variant)?;
        Ok(self.build_enum_construct(&enum_name, variant, tag, payload, span))
    }
}
