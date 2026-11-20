//! Struct literals, generic struct instantiation, and field typing.
//!
//! Reached from the `lower_expr_uncoerced` dispatch in this module's `mod.rs`.
//! Every file here adds methods to the same `impl Lowerer` block.

use ast_types::{BinaryOp, Expr, FieldInit};
use neuro_hir::{HirExpr, HirExprKind, HirFieldInit, HirStmt, HirType};

use crate::extents::{
    assign_local, binary, borrow, extent_read, guard, local_declaration, local_name, panic_with,
    shown, text, variable,
};
use crate::items::written_name;
use crate::{Lowerer, LoweringError};

impl Lowerer {
    pub(super) fn lower_struct_literal(
        &mut self,
        name: &shared_types::Identifier,
        fields: &[FieldInit],
        base: &Option<Box<Expr>>,
        expected: Option<&HirType>,
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        // A generic struct literal infers its type arguments from the field values,
        // then monomorphizes into a concrete instance.
        if self.generic_structs.contains_key(&name.name) {
            return self.lower_generic_struct_literal(name, fields, base, expected, span);
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
        if let Some(names) = self.struct_extents.get(&name.name).cloned() {
            return self.named_struct_literal(
                &name.name,
                name.name.clone(),
                lowered_fields,
                base,
                &names,
                span,
            );
        }

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

    /// A struct literal whose fields name dynamic extents (R6). Each field value is bound to
    /// a temporary in the order it is written, so the values are computed as before. The
    /// struct's names are fresh per literal: the first field naming one binds it, and each
    /// later field naming it is checked against it, in declaration order. Their locals live
    /// in the literal's own block, so they end with it.
    pub(super) fn named_struct_literal(
        &mut self,
        display: &str,
        struct_name: String,
        fields: Vec<HirFieldInit>,
        base: Option<Box<HirExpr>>,
        names: &[crate::extents::FieldExtent],
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        let struct_ty = HirType::Struct(struct_name.clone());
        let mut stmts = Vec::with_capacity(fields.len() + names.len() * 2 + 1);
        let mut temps: Vec<(String, String, HirType)> = Vec::with_capacity(fields.len());
        let mut lowered = Vec::with_capacity(fields.len());
        for field in fields {
            let temp = self.fresh_extent_temp();
            let ty = field.value.ty.clone();
            stmts.push(HirStmt::VarDecl {
                name: temp.clone(),
                ty: ty.clone(),
                init: Some(*field.value),
                mutable: false,
                span: field.span,
            });
            lowered.push(HirFieldInit {
                name: field.name.clone(),
                value: Box::new(variable(&temp, ty.clone(), field.span)),
                span: field.span,
            });
            temps.push((field.name, temp, ty));
        }

        let mut bound: Vec<(String, String)> = Vec::new();
        for entry in names {
            let Some((_, temp, ty)) = temps.iter().find(|(field, _, _)| *field == entry.field)
            else {
                // A field the literal leaves to its base already holds an agreed extent.
                continue;
            };
            let found = extent_read(borrow(variable(temp, ty.clone(), span)), entry.axis, span);
            if let Some((_, first)) = bound.iter().find(|(seen, _)| *seen == entry.name) {
                let local = variable(&local_name(&entry.name), HirType::U64, span);
                let message = vec![
                    text(format!(
                        "dynamic extent `{}` of `{}` is ",
                        entry.name,
                        written_name(display)
                    )),
                    shown(local.clone()),
                    text(format!(" from `{first}` but ")),
                    shown(found.clone()),
                    text(format!(" from `{}`", entry.field)),
                ];
                let agreed = binary(BinaryOp::Equal, found, local, HirType::Bool, span);
                stmts.push(guard(agreed, panic_with(message, span), span));
            } else {
                stmts.push(local_declaration(&entry.name, span));
                stmts.push(assign_local(&entry.name, found, span));
                bound.push((entry.name.clone(), entry.field.clone()));
            }
        }

        stmts.push(HirStmt::Expr(HirExpr::new(
            HirExprKind::StructLiteral {
                name: struct_name,
                fields: lowered,
                base,
            },
            struct_ty.clone(),
            span,
        )));
        Ok(HirExpr::new(HirExprKind::Block { stmts }, struct_ty, span))
    }

    /// Lower a generic struct literal: infer the type arguments by unifying the
    /// template's field annotations against the lowered field values, monomorphize the
    /// instance, and emit an ordinary struct literal referring to its mangled name.
    pub(super) fn lower_generic_struct_literal(
        &mut self,
        name: &shared_types::Identifier,
        fields: &[FieldInit],
        base: &Option<Box<Expr>>,
        expected: Option<&HirType>,
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        let instance_fields = self.expected_struct_instance_fields(&name.name, expected);
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
            .filter(|g| {
                matches!(
                    g.kind,
                    ast_types::GenericParamKind::Type | ast_types::GenericParamKind::Shape
                )
            })
            .map(|g| g.name.name.clone())
            .collect();
        let cnames: std::collections::HashSet<String> = template
            .generics
            .iter()
            .filter(|g| {
                matches!(
                    g.kind,
                    ast_types::GenericParamKind::Const(_) | ast_types::GenericParamKind::Extent
                )
            })
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
            // The instance the context names types the field value, mirroring the
            // checker; without one the value drives inference on its own.
            // A field whose declared type names no parameter (`count: u64`) types its value
            // either way, or an unsuffixed literal there would lower as an `i32`.
            let field_expected = instance_fields
                .as_ref()
                .and_then(|f| f.iter().find(|(n, _)| n == &fname.name))
                .map(|(_, t)| t.clone())
                .or_else(|| {
                    let ft = field_ast_ty.as_ref()?;
                    let unbound = Default::default();
                    if crate::items::mentions_any(
                        ft,
                        &gnames,
                        &cnames,
                        &unbound,
                        &Default::default(),
                    ) {
                        return None;
                    }
                    self.resolve_type(ft).ok()
                });
            let lowered = self.lower_expr(value, field_expected.as_ref())?;
            if let Some(ft) = &field_ast_ty {
                self.unify_ast_hir(
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

        // The instance the context names wins over field inference: the checker required
        // the literal to be that instance, and a parameter no field names (`Tag<T> { n:
        // i32 }`) has no field value to be inferred from.
        let expected_args = self
            .expected_struct_instance(&name.name, expected)
            .and_then(|instance| {
                self.struct_instances
                    .get(instance)
                    .map(|(_, args)| args.clone())
            });
        let args = match expected_args {
            Some(args) => args,
            None => template
                .generics
                .iter()
                .map(|gp| match &gp.kind {
                    ast_types::GenericParamKind::Const(_) | ast_types::GenericParamKind::Extent => {
                        crate::MonoArg::Const(const_subst.get(&gp.name.name).copied().unwrap_or(0))
                    }
                    ast_types::GenericParamKind::Type | ast_types::GenericParamKind::Shape => {
                        crate::MonoArg::Type(
                            subst.get(&gp.name.name).cloned().unwrap_or(HirType::Void),
                        )
                    }
                })
                .collect(),
        };
        let mangled = self.instantiate_generic_struct(&name.name, &args)?;
        let struct_ty = HirType::Struct(mangled.clone());
        let base = match base {
            Some(b) => Some(Box::new(self.lower_expr(b, Some(&struct_ty))?)),
            None => None,
        };
        // Each field naming a dynamic extent agrees with the others when the value is built.
        let names: Vec<crate::extents::FieldExtent> = template
            .fields
            .iter()
            .flat_map(|field| {
                crate::extents::tensor_names(&field.ty)
                    .into_iter()
                    .map(move |(axis, name)| crate::extents::FieldExtent {
                        field: field.name.name.clone(),
                        axis,
                        name,
                    })
            })
            .collect();
        if !names.is_empty() {
            return self.named_struct_literal(
                &name.name,
                mangled,
                lowered_fields,
                base,
                &names,
                span,
            );
        }
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

    /// The concrete fields of the instance of generic struct `base` that `expected`
    /// names, if it names one. The checker has already required the literal to be
    /// that instance, so `W { a: 5 }` under `W<i64>` lowers `5` as an `i64`.
    fn expected_struct_instance_fields(
        &self,
        base: &str,
        expected: Option<&HirType>,
    ) -> Option<Vec<(String, HirType)>> {
        let instance = self.expected_struct_instance(base, expected)?;
        self.structs.get(instance).cloned()
    }

    /// The mangled name of the instance of generic struct `base` that `expected` names,
    /// if it names one.
    fn expected_struct_instance<'e>(
        &self,
        base: &str,
        expected: Option<&'e HirType>,
    ) -> Option<&'e str> {
        let Some(HirType::Struct(instance)) = expected else {
            return None;
        };
        let names_base = instance
            .strip_prefix(base)
            .is_some_and(|rest| rest.starts_with("$g_"));
        (names_base && self.struct_instances.contains_key(instance)).then_some(instance.as_str())
    }

    /// The declared type of `field` on `struct_name`.
    pub(crate) fn struct_field_type(
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
