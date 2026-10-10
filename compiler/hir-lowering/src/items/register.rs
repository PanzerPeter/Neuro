//! The registration pre-pass: every struct, enum, trait, impl, function and constant
//! recorded before any body is lowered, so bodies resolve names regardless of order.

use ast_types::{
    ConstDef, EnumDef, FunctionDef, ImplDef, Item, MethodDef, StructDef, VariantPayload,
};
use neuro_hir::HirType;

use crate::{EnumVariantData, Lowerer, LoweringError};

use super::body_result_expr;

/// The `@derive(...)` attribute name and the trait arguments lowering cares about.
const DERIVE_ATTRIBUTE: &str = "derive";
const COPY_TRAIT: &str = "Copy";
const CLONE_TRAIT: &str = "Clone";
const PARTIAL_EQ_TRAIT: &str = "PartialEq";

impl Lowerer {
    /// Build the global symbol tables (structs, methods, functions, constants) in a
    /// pre-pass so bodies see every item regardless of source order, mirroring the
    /// checker's registration passes.
    pub(crate) fn register_items(&mut self, items: &[Item]) -> Result<(), LoweringError> {
        // Newtype names first so they resolve as struct fields, enum payloads, or
        // other newtypes' inners regardless of source order.
        for item in items {
            if let Item::Newtype(def) = item {
                self.newtypes
                    .insert(def.name.name.clone(), def.inner.clone());
            }
        }
        // Enum NAMES before struct fields resolve, and enum PAYLOADS after: a payload
        // may name a struct and a struct field may name the enum, so neither table can
        // be complete before the other's names exist.
        for item in items {
            if let Item::Enum(def) = item {
                if def.generics.is_empty() {
                    self.enums.insert(def.name.name.clone(), Vec::new());
                } else {
                    self.generic_enums
                        .insert(def.name.name.clone(), def.clone());
                }
            }
        }
        for item in items {
            if let Item::Struct(def) = item {
                if def.generics.is_empty() {
                    self.register_struct(def)?;
                } else {
                    self.register_generic_struct(def);
                }
            }
        }
        for item in items {
            if let Item::Enum(def) = item
                && def.generics.is_empty()
            {
                self.register_enum(def)?;
            }
        }
        // Traits before impls: an `impl Trait for T` and a `&dyn Trait` annotation both
        // resolve against the trait's declaration-ordered method list.
        for item in items {
            if let Item::Trait(def) = item {
                self.register_trait(def)?;
            }
        }
        for item in items {
            if let Item::Impl(def) = item {
                if def.generics.is_empty() && def.type_args.is_empty() {
                    self.register_impl(def)?;
                } else {
                    self.generic_impls
                        .entry(def.type_name.name.clone())
                        .or_default()
                        .push(def.clone());
                }
            }
        }
        self.register_existing_enum_instance_methods()?;
        for item in items {
            match item {
                Item::Function(func) => self.register_function(func)?,
                Item::Const(def) => self.register_const(def)?,
                _ => {}
            }
        }
        Ok(())
    }

    pub(super) fn register_struct(&mut self, def: &StructDef) -> Result<(), LoweringError> {
        let mut fields = Vec::with_capacity(def.fields.len());
        for field in &def.fields {
            fields.push((field.name.name.clone(), self.resolve_type(&field.ty)?));
        }
        self.structs.insert(def.name.name.clone(), fields);

        let (mut copy, mut clone, mut partial_eq) = (false, false, false);
        for attr in &def.attributes {
            if attr.name.name != DERIVE_ATTRIBUTE {
                continue;
            }
            for arg in &attr.args {
                match arg.name.as_str() {
                    COPY_TRAIT => copy = true,
                    CLONE_TRAIT => clone = true,
                    PARTIAL_EQ_TRAIT => partial_eq = true,
                    _ => {}
                }
            }
        }
        // `Copy` implies `Clone`: a Copy type is trivially cloneable.
        if copy || clone {
            self.clone_structs.insert(def.name.name.clone());
        }
        if partial_eq {
            self.partial_eq_structs.insert(def.name.name.clone());
        }
        Ok(())
    }

    /// Register a generic struct template. Only the template is recorded; each
    /// distinct set of type arguments is monomorphized on demand. Clone/Copy intent is
    /// recorded under the base name so instances can inherit `.clone()` support.
    pub(super) fn register_generic_struct(&mut self, def: &StructDef) {
        let mut clone = false;
        let mut partial_eq = false;
        for attr in &def.attributes {
            if attr.name.name != DERIVE_ATTRIBUTE {
                continue;
            }
            for arg in &attr.args {
                match arg.name.as_str() {
                    COPY_TRAIT | CLONE_TRAIT => clone = true,
                    PARTIAL_EQ_TRAIT => partial_eq = true,
                    _ => {}
                }
            }
        }
        if clone {
            self.clone_structs.insert(def.name.name.clone());
        }
        if partial_eq {
            self.partial_eq_structs.insert(def.name.name.clone());
        }
        self.generic_structs
            .insert(def.name.name.clone(), def.clone());
    }

    /// Resolve an enum's variants and payload field types into the lowering table
    /// Mirrors the checker's registration; payload-type Copy/scalar
    /// validation is the checker's job and not repeated here.
    pub(super) fn register_enum(&mut self, def: &EnumDef) -> Result<(), LoweringError> {
        let variants = self.resolve_variants(def)?;
        self.enums.insert(def.name.name.clone(), variants);
        Ok(())
    }

    /// Resolve every variant's payload types under the active substitution, so a
    /// generic template resolves to concrete payloads inside an instantiation.
    pub(super) fn resolve_variants(
        &mut self,
        def: &EnumDef,
    ) -> Result<Vec<EnumVariantData>, LoweringError> {
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
        Ok(variants)
    }

    /// Record a trait's methods in declaration order. That order is the vtable
    /// slot order every implementor shares, and the signatures let a call through a
    /// `&dyn Trait` receiver be typed without naming a concrete implementor.
    pub(super) fn register_trait(
        &mut self,
        def: &ast_types::TraitDef,
    ) -> Result<(), LoweringError> {
        let mut methods = Vec::with_capacity(def.methods.len());
        for method in &def.methods {
            let mut params = Vec::with_capacity(method.params.len());
            for param in &method.params {
                params.push(self.resolve_trait_sig_type(&param.ty)?);
            }
            let ret = match &method.return_type {
                Some(t) => self.resolve_trait_sig_type(t)?,
                None => HirType::Void,
            };
            methods.push(crate::TraitMethodInfo {
                name: method.name.name.clone(),
                params,
                ret,
            });
        }
        self.traits.insert(def.name.name.clone(), methods);
        Ok(())
    }

    pub(super) fn register_impl(&mut self, def: &ImplDef) -> Result<(), LoweringError> {
        let struct_name = def.type_name.name.clone();
        if let Some(trait_name) = &def.trait_name {
            self.trait_impls
                .insert((trait_name.name.clone(), struct_name.clone()));
        }
        let saved_ty = self.enter_impl_assoc(def)?;
        let outcome = self.register_impl_methods(def, &struct_name);
        self.type_subst = saved_ty;
        outcome
    }

    pub(super) fn register_impl_methods(
        &mut self,
        def: &ImplDef,
        struct_name: &str,
    ) -> Result<(), LoweringError> {
        for method in &def.methods {
            // An owned `self` on a `Copy` receiver is valid for operator-trait methods
            // The checker already rejected it on any non-`Copy` type, so every
            // owned-`self` method reaching lowering is sound and is registered normally.
            let mangled = format!("{}__{}", struct_name, method.name.name);
            let self_ty = self.impl_target_type(struct_name)?;
            let (params, ret) = self.method_signature(&self_ty, method)?;
            self.functions.insert(mangled.clone(), (params, ret));
            if crate::autodiff::is_grad(&method.attributes) {
                let names = method.params.iter().map(|p| p.name.name.clone()).collect();
                let grad = crate::autodiff::GradParams::of(names, &method.attributes)?;
                self.grad_params.insert(mangled.clone(), grad);
            }
            self.impl_methods
                .entry(struct_name.to_string())
                .or_default()
                .insert(method.name.name.clone(), mangled);
        }
        self.register_operator_impl(def, struct_name)?;
        Ok(())
    }

    /// Install an `impl` block's associated-type bindings, returning the substitution to
    /// restore afterwards.
    ///
    /// A binding is a name standing for a concrete type over one block, which is what
    /// the type-parameter substitution already is, so `Self::Item` joins it under its
    /// written spelling and every annotation resolves through the one path.
    pub(super) fn enter_impl_assoc(
        &mut self,
        def: &ImplDef,
    ) -> Result<std::collections::HashMap<String, HirType>, LoweringError> {
        if def.assoc_types.is_empty() {
            return Ok(self.type_subst.clone());
        }
        let mut subst = self.type_subst.clone();
        for (name, ty) in &def.assoc_types {
            let resolved = self.resolve_type(ty)?;
            subst.insert(format!("Self::{}", name.name), resolved);
        }
        Ok(std::mem::replace(&mut self.type_subst, subst))
    }

    /// Record the operator dispatch for an operator-trait impl, mirroring the
    /// checker so this slice resolves operators on user types independently.
    pub(super) fn register_operator_impl(
        &mut self,
        def: &ImplDef,
        struct_name: &str,
    ) -> Result<(), LoweringError> {
        let Some(trait_name) = def.trait_name.as_ref().map(|t| &t.name) else {
            return Ok(());
        };
        let Some(spec) = crate::operator_traits::operator_trait_spec(trait_name) else {
            return Ok(());
        };
        for method in &def.methods {
            let ret = match &method.return_type {
                Some(t) => self.resolve_type(t)?,
                None => HirType::Void,
            };
            let result = if spec.has_output { ret } else { HirType::Bool };
            if let Some((_, op)) = spec.binary.iter().find(|(m, _)| *m == method.name.name) {
                let rhs_param = match method.params.first() {
                    Some(p) => self.resolve_type(&p.ty)?,
                    None => HirType::Void,
                };
                self.operator_binary_impls.insert(
                    (struct_name.to_string(), *op),
                    crate::OpDispatch {
                        method: method.name.name.clone(),
                        rhs_param,
                        result,
                    },
                );
            } else if let Some((_, op)) = spec.unary.iter().find(|(m, _)| *m == method.name.name) {
                self.operator_unary_impls.insert(
                    (struct_name.to_string(), *op),
                    (method.name.name.clone(), result),
                );
            }
        }
        Ok(())
    }

    /// The full signature of a method: the implicit `self` (the impl's target type)
    /// leads the parameter list for an instance method, then the declared parameters.
    pub(super) fn method_signature(
        &mut self,
        self_ty: &HirType,
        method: &MethodDef,
    ) -> Result<(Vec<HirType>, HirType), LoweringError> {
        let mut params = Vec::new();
        if method.self_param.is_some() {
            params.push(self_ty.clone());
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

    pub(super) fn register_function(&mut self, func: &FunctionDef) -> Result<(), LoweringError> {
        // A generic function is a template, not a callable signature: record it
        // for monomorphization and skip the concrete-signature registration below.
        if !func.generics.is_empty() {
            self.generic_templates
                .insert(func.name.name.clone(), func.clone());
            return Ok(());
        }

        let mut params = Vec::with_capacity(func.params.len());
        for param in &func.params {
            params.push(self.resolve_type(&param.ty)?);
        }
        let ret = self.declared_return_type(&func.return_type, &func.body)?;
        self.functions.insert(func.name.name.clone(), (params, ret));
        if crate::autodiff::is_grad(&func.attributes) {
            let grad = crate::autodiff::GradParams::of(crate::param_names(func), &func.attributes)?;
            self.grad_params.insert(func.name.name.clone(), grad);
        }
        Ok(())
    }

    /// The resolved return type of a function, resolving return-position `impl Trait`
    /// to the concrete type the body constructs.
    ///
    /// `impl Trait` in return position is static dispatch, since exactly one concrete type
    /// leaves the function, so it is transparent here, exactly as the checker resolved
    /// it. The concrete type is read structurally from the body's result expression; the
    /// checker has already verified it exists and implements the trait.
    pub(crate) fn declared_return_type(
        &mut self,
        return_type: &Option<ast_types::Type>,
        body: &[ast_types::Stmt],
    ) -> Result<HirType, LoweringError> {
        match return_type {
            Some(ast_types::Type::ImplTrait { trait_name, .. }) => {
                match body_result_expr(body).and_then(|e| self.shallow_result_type(e)) {
                    Some(ty) => Ok(ty),
                    None => Err(LoweringError::UnresolvedType {
                        name: format!("impl {}", trait_name.name),
                    }),
                }
            }
            Some(t) => self.resolve_type(t),
            None => Ok(HirType::Void),
        }
    }

    /// The concrete type of a directly-constructed expression, read structurally
    /// Mirrors the checker's inference for return-position `impl Trait`;
    /// duplicated rather than shared because the two slices own separate type tables.
    pub(super) fn shallow_result_type(&mut self, expr: &ast_types::Expr) -> Option<HirType> {
        use ast_types::Expr;
        match expr {
            Expr::Paren(inner, _) => self.shallow_result_type(inner),
            Expr::StructLiteral { name, .. } => self
                .structs
                .contains_key(&name.name)
                .then(|| HirType::Struct(name.name.clone())),
            Expr::EnumStructLiteral { enum_name, .. } => self
                .enums
                .contains_key(&enum_name.name)
                .then(|| HirType::Enum(enum_name.name.clone())),
            Expr::Path { type_name, .. } => self
                .enums
                .contains_key(&type_name.name)
                .then(|| HirType::Enum(type_name.name.clone())),
            Expr::Call { func, .. } => match func.as_ref() {
                Expr::Path { type_name, .. } => self
                    .enums
                    .contains_key(&type_name.name)
                    .then(|| HirType::Enum(type_name.name.clone())),
                Expr::Identifier(ident) => {
                    let inner_ast = self.newtypes.get(&ident.name)?.clone();
                    let inner = self.resolve_type(&inner_ast).ok()?;
                    Some(HirType::Newtype {
                        name: ident.name.clone(),
                        inner: Box::new(inner),
                    })
                }
                _ => None,
            },
            Expr::Block { stmts, .. } => {
                body_result_expr(stmts).and_then(|e| self.shallow_result_type(e))
            }
            Expr::If { then_block, .. } => {
                body_result_expr(then_block).and_then(|e| self.shallow_result_type(e))
            }
            _ => None,
        }
    }

    pub(super) fn register_const(&mut self, def: &ConstDef) -> Result<(), LoweringError> {
        let ty = self.resolve_type(&def.ty)?;
        self.constants.insert(def.name.name.clone(), ty);
        self.constant_values
            .insert(def.name.name.clone(), def.value.clone());
        Ok(())
    }

    /// The receiver type of an `impl` on `name`: an enum when the name is a declared
    /// enum, a generic enum template or an instance of one, a struct otherwise.
    pub(crate) fn impl_target_type(&mut self, name: &str) -> Result<HirType, LoweringError> {
        if self.enums.contains_key(name) || self.generic_enums.contains_key(name) {
            return Ok(HirType::Enum(name.to_string()));
        }
        if let Some(inner) = self.newtypes.get(name).cloned() {
            return Ok(HirType::Newtype {
                name: name.to_string(),
                inner: Box::new(self.resolve_type(&inner)?),
            });
        }
        Ok(HirType::Struct(name.to_string()))
    }

    /// The generic parameters of the struct or enum template named `base`.
    pub(crate) fn template_generics(&self, base: &str) -> Vec<ast_types::GenericParam> {
        if let Some(s) = self.generic_structs.get(base) {
            return s.generics.clone();
        }
        self.generic_enums
            .get(base)
            .map(|e| e.generics.clone())
            .unwrap_or_default()
    }

    /// Resolve each const generic parameter's declared integer type, so a value
    /// reference to one in a monomorphized body lowers to a correctly-typed literal.
    pub(super) fn const_param_types(
        &mut self,
        generics: &[ast_types::GenericParam],
    ) -> Result<std::collections::HashMap<String, HirType>, LoweringError> {
        let mut out = std::collections::HashMap::new();
        for gp in generics {
            if let ast_types::GenericParamKind::Const(ty) = &gp.kind {
                out.insert(gp.name.name.clone(), self.resolve_type(ty)?);
            }
        }
        Ok(out)
    }
}
