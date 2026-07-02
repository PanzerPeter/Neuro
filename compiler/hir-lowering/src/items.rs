//! Top-level item registration and lowering.

use ast_types::{
    ConstDef, EnumDef, FunctionDef, ImplDef, Item, MethodDef, SelfParam, StructDef, VariantPayload,
};
use neuro_hir::{
    HirConst, HirEnum, HirEnumField, HirEnumVariant, HirField, HirFunction, HirImpl, HirItem,
    HirMethod, HirParam, HirProgram, HirSelfParam, HirStmt, HirStruct, HirType,
};

use crate::{EnumVariantData, Lowerer, LoweringError};

/// The `@derive(...)` attribute name and the trait arguments lowering cares about.
const DERIVE_ATTRIBUTE: &str = "derive";
const COPY_TRAIT: &str = "Copy";
const CLONE_TRAIT: &str = "Clone";

impl Lowerer {
    /// Build the global symbol tables (structs, methods, functions, constants) in a
    /// pre-pass so bodies see every item regardless of source order — mirroring the
    /// checker's registration passes.
    pub(crate) fn register_items(&mut self, items: &[Item]) -> Result<(), LoweringError> {
        // Newtype names first so they resolve as struct fields, enum payloads, or
        // other newtypes' inners regardless of source order (§3.15).
        for item in items {
            if let Item::Newtype(def) = item {
                self.newtypes
                    .insert(def.name.name.clone(), def.inner.clone());
            }
        }
        for item in items {
            if let Item::Enum(def) = item {
                self.register_enum(def)?;
            }
        }
        for item in items {
            if let Item::Struct(def) = item {
                self.register_struct(def)?;
            }
        }
        for item in items {
            if let Item::Impl(def) = item {
                self.register_impl(def)?;
            }
        }
        for item in items {
            match item {
                Item::Function(func) => self.register_function(func)?,
                Item::Const(def) => self.register_const(def)?,
                _ => {}
            }
        }
        Ok(())
    }

    fn register_struct(&mut self, def: &StructDef) -> Result<(), LoweringError> {
        let mut fields = Vec::with_capacity(def.fields.len());
        for field in &def.fields {
            fields.push((field.name.name.clone(), self.resolve_type(&field.ty)?));
        }
        self.structs.insert(def.name.name.clone(), fields);

        let (mut copy, mut clone) = (false, false);
        for attr in &def.attributes {
            if attr.name.name != DERIVE_ATTRIBUTE {
                continue;
            }
            for arg in &attr.args {
                match arg.name.as_str() {
                    COPY_TRAIT => copy = true,
                    CLONE_TRAIT => clone = true,
                    _ => {}
                }
            }
        }
        // `Copy` implies `Clone` (§2.3): a Copy type is trivially cloneable.
        if copy || clone {
            self.clone_structs.insert(def.name.name.clone());
        }
        Ok(())
    }

    /// Resolve an enum's variants and payload field types into the lowering table
    /// (§3.5). Mirrors the checker's registration; payload-type Copy/scalar
    /// validation is the checker's job and not repeated here.
    fn register_enum(&mut self, def: &EnumDef) -> Result<(), LoweringError> {
        let mut variants = Vec::with_capacity(def.variants.len());
        for variant in &def.variants {
            let fields = match &variant.payload {
                VariantPayload::Unit => Vec::new(),
                VariantPayload::Tuple(tys) => {
                    let mut fields = Vec::with_capacity(tys.len());
                    for ty in tys {
                        fields.push((None, self.resolve_type(ty)?));
                    }
                    fields
                }
                VariantPayload::Struct(field_defs) => {
                    let mut fields = Vec::with_capacity(field_defs.len());
                    for field in field_defs {
                        fields.push((Some(field.name.name.clone()), self.resolve_type(&field.ty)?));
                    }
                    fields
                }
            };
            variants.push(EnumVariantData {
                name: variant.name.name.clone(),
                fields,
            });
        }
        self.enums.insert(def.name.name.clone(), variants);
        Ok(())
    }

    fn register_impl(&mut self, def: &ImplDef) -> Result<(), LoweringError> {
        let struct_name = def.type_name.name.clone();
        for method in &def.methods {
            // Consuming `self` is rejected by the checker and never reaches lowering.
            if matches!(method.self_param, Some(SelfParam::Owned)) {
                continue;
            }
            let mangled = format!("{}__{}", struct_name, method.name.name);
            let (params, ret) = self.method_signature(&struct_name, method)?;
            self.functions.insert(mangled.clone(), (params, ret));
            self.impl_methods
                .entry(struct_name.clone())
                .or_default()
                .insert(method.name.name.clone(), mangled);
        }
        Ok(())
    }

    /// The full signature of a method: the implicit `self` (the struct type) leads
    /// the parameter list for an instance method, then the declared parameters.
    fn method_signature(
        &self,
        struct_name: &str,
        method: &MethodDef,
    ) -> Result<(Vec<HirType>, HirType), LoweringError> {
        let mut params = Vec::new();
        if method.self_param.is_some() {
            params.push(HirType::Struct(struct_name.to_string()));
        }
        for param in &method.params {
            params.push(self.resolve_type(&param.ty)?);
        }
        let ret = match &method.return_type {
            Some(t) => self.resolve_type(t)?,
            None => HirType::Void,
        };
        Ok((params, ret))
    }

    fn register_function(&mut self, func: &FunctionDef) -> Result<(), LoweringError> {
        let mut params = Vec::with_capacity(func.params.len());
        for param in &func.params {
            params.push(self.resolve_type(&param.ty)?);
        }
        let ret = match &func.return_type {
            Some(t) => self.resolve_type(t)?,
            None => HirType::Void,
        };
        self.functions.insert(func.name.name.clone(), (params, ret));
        Ok(())
    }

    fn register_const(&mut self, def: &ConstDef) -> Result<(), LoweringError> {
        let ty = self.resolve_type(&def.ty)?;
        self.constants.insert(def.name.name.clone(), ty);
        Ok(())
    }

    /// Lower every top-level item to its HIR form.
    pub(crate) fn lower_program(&mut self, items: &[Item]) -> Result<HirProgram, LoweringError> {
        let mut hir_items = Vec::with_capacity(items.len());
        for item in items {
            match item {
                Item::Function(func) => {
                    hir_items.push(HirItem::Function(self.lower_function(func)?))
                }
                Item::Struct(def) => hir_items.push(HirItem::Struct(self.lower_struct(def)?)),
                Item::Enum(def) => hir_items.push(HirItem::Enum(self.lower_enum(def)?)),
                Item::Impl(def) => hir_items.push(HirItem::Impl(self.lower_impl(def)?)),
                Item::Const(def) => hir_items.push(HirItem::Const(self.lower_const(def)?)),
                // A newtype is transparent at runtime and produces no HIR item; it
                // survives only as the `HirType::Newtype` its annotations resolve to.
                Item::Newtype(_) => {}
            }
        }
        Ok(HirProgram { items: hir_items })
    }

    fn lower_struct(&self, def: &StructDef) -> Result<HirStruct, LoweringError> {
        let mut fields = Vec::with_capacity(def.fields.len());
        for field in &def.fields {
            fields.push(HirField {
                name: field.name.name.clone(),
                ty: self.resolve_type(&field.ty)?,
                span: field.span,
            });
        }
        Ok(HirStruct {
            name: def.name.name.clone(),
            fields,
            span: def.span,
        })
    }

    fn lower_enum(&self, def: &EnumDef) -> Result<HirEnum, LoweringError> {
        let mut variants = Vec::with_capacity(def.variants.len());
        for variant in &def.variants {
            let fields = match &variant.payload {
                VariantPayload::Unit => Vec::new(),
                VariantPayload::Tuple(tys) => {
                    let mut fields = Vec::with_capacity(tys.len());
                    for ty in tys {
                        fields.push(HirEnumField {
                            name: None,
                            ty: self.resolve_type(ty)?,
                        });
                    }
                    fields
                }
                VariantPayload::Struct(field_defs) => {
                    let mut fields = Vec::with_capacity(field_defs.len());
                    for field in field_defs {
                        fields.push(HirEnumField {
                            name: Some(field.name.name.clone()),
                            ty: self.resolve_type(&field.ty)?,
                        });
                    }
                    fields
                }
            };
            variants.push(HirEnumVariant {
                name: variant.name.name.clone(),
                fields,
                span: variant.span,
            });
        }
        Ok(HirEnum {
            name: def.name.name.clone(),
            variants,
            span: def.span,
        })
    }

    fn lower_const(&mut self, def: &ConstDef) -> Result<HirConst, LoweringError> {
        let ty = self.resolve_type(&def.ty)?;
        let value = self.lower_expr(&def.value, Some(&ty))?;
        Ok(HirConst {
            name: def.name.name.clone(),
            ty,
            value,
            span: def.span,
        })
    }

    fn lower_function(&mut self, func: &FunctionDef) -> Result<HirFunction, LoweringError> {
        let mut params = Vec::with_capacity(func.params.len());
        for param in &func.params {
            params.push(HirParam {
                name: param.name.name.clone(),
                ty: self.resolve_type(&param.ty)?,
                span: param.span,
            });
        }
        let return_type = match &func.return_type {
            Some(t) => self.resolve_type(t)?,
            None => HirType::Void,
        };

        self.push_scope();
        for param in &params {
            self.define(param.name.clone(), param.ty.clone());
        }
        let body = self.lower_body(&func.body, &return_type)?;
        self.pop_scope();

        Ok(HirFunction {
            name: func.name.name.clone(),
            params,
            return_type,
            body,
            span: func.span,
        })
    }

    fn lower_impl(&mut self, def: &ImplDef) -> Result<HirImpl, LoweringError> {
        let struct_name = def.type_name.name.clone();
        let mut methods = Vec::new();
        for method in &def.methods {
            if matches!(method.self_param, Some(SelfParam::Owned)) {
                continue;
            }
            methods.push(self.lower_method(&struct_name, method)?);
        }
        Ok(HirImpl {
            type_name: struct_name,
            trait_name: def.trait_name.as_ref().map(|t| t.name.clone()),
            methods,
            span: def.span,
        })
    }

    fn lower_method(
        &mut self,
        struct_name: &str,
        method: &MethodDef,
    ) -> Result<HirMethod, LoweringError> {
        let mut params = Vec::with_capacity(method.params.len());
        for param in &method.params {
            params.push(HirParam {
                name: param.name.name.clone(),
                ty: self.resolve_type(&param.ty)?,
                span: param.span,
            });
        }
        let return_type = match &method.return_type {
            Some(t) => self.resolve_type(t)?,
            None => HirType::Void,
        };
        let self_param = method.self_param.as_ref().map(lower_self_param);

        self.push_scope();
        if self_param.is_some() {
            self.define("self".to_string(), HirType::Struct(struct_name.to_string()));
        }
        for param in &params {
            self.define(param.name.clone(), param.ty.clone());
        }
        let body = self.lower_body(&method.body, &return_type)?;
        self.pop_scope();

        Ok(HirMethod {
            name: method.name.name.clone(),
            self_param,
            params,
            return_type,
            body,
            span: method.span,
        })
    }

    /// Lower a function/method body. The trailing expression of a non-`void` body is
    /// an implicit return, so it is typed against the declared return type — exactly
    /// the contextual hint the checker applies (§1.8); every other statement lowers
    /// with no expected type.
    pub(crate) fn lower_body(
        &mut self,
        body: &[ast_types::Stmt],
        return_type: &HirType,
    ) -> Result<Vec<HirStmt>, LoweringError> {
        let mut out = Vec::with_capacity(body.len());
        let last = body.len().saturating_sub(1);
        for (i, stmt) in body.iter().enumerate() {
            let is_tail = i == last;
            if is_tail && !matches!(return_type, HirType::Void) {
                if let ast_types::Stmt::Expr(expr) = stmt {
                    out.push(HirStmt::Expr(self.lower_expr(expr, Some(return_type))?));
                    continue;
                }
            }
            out.push(self.lower_stmt(stmt)?);
        }
        Ok(out)
    }
}

/// Lower the surface `self` receiver kind to its HIR counterpart.
fn lower_self_param(sp: &SelfParam) -> HirSelfParam {
    match sp {
        SelfParam::Ref => HirSelfParam::Ref,
        SelfParam::RefMut => HirSelfParam::RefMut,
        SelfParam::Owned => HirSelfParam::Owned,
    }
}
