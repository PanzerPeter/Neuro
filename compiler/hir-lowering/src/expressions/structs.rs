//! Struct literals, generic struct instantiation, and field typing.
//!
//! Reached from the `lower_expr_uncoerced` dispatch in this module's `mod.rs`.
//! Every file here adds methods to the same `impl Lowerer` block.

use ast_types::{Expr, FieldInit};
use neuro_hir::{HirExpr, HirExprKind, HirFieldInit, HirType};

use crate::{Lowerer, LoweringError};

impl Lowerer {
    pub(super) fn lower_struct_literal(
        &mut self,
        name: &shared_types::Identifier,
        fields: &[FieldInit],
        base: &Option<Box<Expr>>,
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        // A generic struct literal infers its type arguments from the field values,
        // then monomorphizes into a concrete instance.
        if self.generic_structs.contains_key(&name.name) {
            return self.lower_generic_struct_literal(name, fields, base, span);
        }

        let def =
            self.structs
                .get(&name.name)
                .cloned()
                .ok_or_else(|| LoweringError::UnresolvedType {
                    name: name.name.clone(),
                })?;

        let mut lowered_fields = Vec::with_capacity(fields.len());
        for FieldInit {
            name: fname,
            value,
            span: fspan,
        } in fields
        {
            let expected = def
                .iter()
                .find(|(n, _)| n == &fname.name)
                .map(|(_, t)| t.clone());
            let value = self.lower_expr(value, expected.as_ref())?;
            lowered_fields.push(HirFieldInit {
                name: fname.name.clone(),
                value: Box::new(value),
                span: *fspan,
            });
        }

        let struct_ty = HirType::Struct(name.name.clone());
        let base = match base {
            Some(b) => Some(Box::new(self.lower_expr(b, Some(&struct_ty))?)),
            None => None,
        };

        Ok(HirExpr::new(
            HirExprKind::StructLiteral {
                name: name.name.clone(),
                fields: lowered_fields,
                base,
            },
            struct_ty,
            span,
        ))
    }

    /// Lower a generic struct literal: infer the type arguments by unifying the
    /// template's field annotations against the lowered field values, monomorphize the
    /// instance, and emit an ordinary struct literal referring to its mangled name.
    pub(super) fn lower_generic_struct_literal(
        &mut self,
        name: &shared_types::Identifier,
        fields: &[FieldInit],
        base: &Option<Box<Expr>>,
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        let template = self
            .generic_structs
            .get(&name.name)
            .cloned()
            .ok_or_else(|| LoweringError::UnresolvedType {
                name: name.name.clone(),
            })?;
        let gnames: std::collections::HashSet<String> = template
            .generics
            .iter()
            .filter(|g| matches!(g.kind, ast_types::GenericParamKind::Type))
            .map(|g| g.name.name.clone())
            .collect();
        let cnames: std::collections::HashSet<String> = template
            .generics
            .iter()
            .filter(|g| matches!(g.kind, ast_types::GenericParamKind::Const(_)))
            .map(|g| g.name.name.clone())
            .collect();

        let mut subst: std::collections::HashMap<String, HirType> =
            std::collections::HashMap::new();
        let mut const_subst: std::collections::HashMap<String, u64> =
            std::collections::HashMap::new();
        let mut lowered_fields = Vec::with_capacity(fields.len());
        for FieldInit {
            name: fname,
            value,
            span: fspan,
        } in fields
        {
            let field_ast_ty = template
                .fields
                .iter()
                .find(|f| f.name.name == fname.name)
                .map(|f| f.ty.clone());
            let lowered = self.lower_expr(value, None)?;
            if let Some(ft) = &field_ast_ty {
                crate::unify_ast_hir(
                    ft,
                    &lowered.ty,
                    &gnames,
                    &cnames,
                    &mut subst,
                    &mut const_subst,
                );
            }
            lowered_fields.push(HirFieldInit {
                name: fname.name.clone(),
                value: Box::new(lowered),
                span: *fspan,
            });
        }

        let mut args = Vec::with_capacity(template.generics.len());
        for gp in &template.generics {
            match &gp.kind {
                ast_types::GenericParamKind::Const(_) => args.push(crate::MonoArg::Const(
                    const_subst.get(&gp.name.name).copied().unwrap_or(0),
                )),
                ast_types::GenericParamKind::Type => args.push(crate::MonoArg::Type(
                    subst.get(&gp.name.name).cloned().unwrap_or(HirType::Void),
                )),
            }
        }
        let mangled = self.instantiate_generic_struct(&name.name, &args)?;
        let struct_ty = HirType::Struct(mangled.clone());
        let base = match base {
            Some(b) => Some(Box::new(self.lower_expr(b, Some(&struct_ty))?)),
            None => None,
        };
        Ok(HirExpr::new(
            HirExprKind::StructLiteral {
                name: mangled,
                fields: lowered_fields,
                base,
            },
            struct_ty,
            span,
        ))
    }

    /// The declared type of `field` on `struct_name`.
    pub(super) fn struct_field_type(
        &self,
        struct_name: &str,
        field: &str,
    ) -> Result<HirType, LoweringError> {
        self.structs
            .get(struct_name)
            .and_then(|fields| fields.iter().find(|(n, _)| n == field))
            .map(|(_, t)| t.clone())
            .ok_or_else(|| LoweringError::Malformed {
                detail: format!("unknown field '{}' on struct '{}'", field, struct_name),
            })
    }
}
