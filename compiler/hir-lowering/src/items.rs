//! Top-level item registration and lowering.

use ast_types::{
    ConstDef, EnumDef, FunctionDef, ImplDef, Item, MethodDef, SelfParam, StructDef, VariantPayload,
};
use neuro_hir::{
    HirConst, HirEnum, HirEnumField, HirEnumVariant, HirField, HirFunction, HirImpl, HirItem,
    HirMethod, HirParam, HirProgram, HirSelfParam, HirStmt, HirStruct, HirType,
};

use crate::{EnumVariantData, Lowerer, LoweringError, MonoInstance};

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
        // other newtypes' inners regardless of source order.
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
                if def.generics.is_empty() {
                    self.register_struct(def)?;
                } else {
                    self.register_generic_struct(def);
                }
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
        // `Copy` implies `Clone`: a Copy type is trivially cloneable.
        if copy || clone {
            self.clone_structs.insert(def.name.name.clone());
        }
        Ok(())
    }

    /// Register a generic struct template. Only the template is recorded; each
    /// distinct set of type arguments is monomorphized on demand. Clone/Copy intent is
    /// recorded under the base name so instances can inherit `.clone()` support.
    fn register_generic_struct(&mut self, def: &StructDef) {
        let mut clone = false;
        for attr in &def.attributes {
            if attr.name.name != DERIVE_ATTRIBUTE {
                continue;
            }
            for arg in &attr.args {
                if matches!(arg.name.as_str(), COPY_TRAIT | CLONE_TRAIT) {
                    clone = true;
                }
            }
        }
        if clone {
            self.clone_structs.insert(def.name.name.clone());
        }
        self.generic_structs
            .insert(def.name.name.clone(), def.clone());
    }

    /// Materialize a monomorphized generic-struct instance: register its
    /// concrete fields and impl-method signatures, queue its HIR items for emission,
    /// and return the mangled instance name. Idempotent per instance.
    pub(crate) fn instantiate_generic_struct(
        &mut self,
        base: &str,
        args: &[crate::MonoArg],
    ) -> Result<String, LoweringError> {
        let template = match self.generic_structs.get(base) {
            Some(t) => t.clone(),
            None => {
                return Err(LoweringError::UnresolvedType {
                    name: base.to_string(),
                })
            }
        };
        let mangled = crate::mangle_struct_instance(base, args);
        if self.instantiated_structs.insert(mangled.clone()) {
            let (subst, const_subst) = split_mono_args(&template.generics, args);

            let saved_ty = std::mem::replace(&mut self.type_subst, subst.clone());
            let saved_c = std::mem::replace(&mut self.const_subst, const_subst.clone());
            let mut fields = Vec::with_capacity(template.fields.len());
            for field in &template.fields {
                fields.push((field.name.name.clone(), self.resolve_type(&field.ty)?));
            }
            self.type_subst = saved_ty;
            self.const_subst = saved_c;
            self.structs.insert(mangled.clone(), fields);

            if self.clone_structs.contains(base) {
                self.clone_structs.insert(mangled.clone());
            }

            self.register_instance_methods(base, &mangled, &subst, &const_subst)?;
            self.mono_struct_pending.push(crate::MonoStruct {
                base: base.to_string(),
                mangled: mangled.clone(),
                subst,
                const_subst,
            });
        }
        Ok(mangled)
    }

    /// Map an impl's type parameters to the struct instance's concrete types:
    /// `impl<T> Wrapper<T>` with instance `Wrapper<i32>` binds `T` → `i32`. The impl's
    /// positional type arguments correspond to the struct's generic parameters, whose
    /// concrete types are read from `subst`. Const parameters carry no impl type binding.
    fn build_impl_subst(
        &self,
        imp: &ImplDef,
        base_generics: &[ast_types::GenericParam],
        subst: &std::collections::HashMap<String, HirType>,
    ) -> std::collections::HashMap<String, HirType> {
        let mut result = std::collections::HashMap::new();
        for (ta, gp) in imp.type_args.iter().zip(base_generics) {
            if let ast_types::Type::Named(id) = ta {
                if imp.generics.iter().any(|g| g.name.name == id.name) {
                    if let Some(concrete) = subst.get(&gp.name.name) {
                        result.insert(id.name.clone(), concrete.clone());
                    }
                }
            }
        }
        result
    }

    /// Register the signature of every method of each generic `impl` of `base` for the
    /// concrete instance `mangled`, so calls on the instance resolve.
    fn register_instance_methods(
        &mut self,
        base: &str,
        mangled: &str,
        subst: &std::collections::HashMap<String, HirType>,
        const_subst: &std::collections::HashMap<String, u64>,
    ) -> Result<(), LoweringError> {
        let impls = self.generic_impls.get(base).cloned().unwrap_or_default();
        let base_generics = self
            .generic_structs
            .get(base)
            .map(|s| s.generics.clone())
            .unwrap_or_default();
        for imp in &impls {
            let impl_subst = self.build_impl_subst(imp, &base_generics, subst);
            for method in &imp.methods {
                if matches!(method.self_param, Some(SelfParam::Owned)) {
                    continue;
                }
                let inst_key = format!("{}__{}", mangled, method.name.name);
                if self.functions.contains_key(&inst_key) {
                    continue;
                }
                let saved_ty = std::mem::replace(&mut self.type_subst, impl_subst.clone());
                let saved_c = std::mem::replace(&mut self.const_subst, const_subst.clone());
                let mut params = Vec::new();
                if method.self_param.is_some() {
                    params.push(HirType::Struct(mangled.to_string()));
                }
                for param in &method.params {
                    params.push(self.resolve_type(&param.ty)?);
                }
                let ret = match &method.return_type {
                    Some(t) => self.resolve_type(t)?,
                    None => HirType::Void,
                };
                self.type_subst = saved_ty;
                self.const_subst = saved_c;
                self.functions.insert(inst_key.clone(), (params, ret));
                self.impl_methods
                    .entry(mangled.to_string())
                    .or_default()
                    .insert(method.name.name.clone(), inst_key);
            }
        }
        Ok(())
    }

    /// Resolve each const generic parameter's declared integer type, so a value
    /// reference to one in a monomorphized body lowers to a correctly-typed literal.
    fn const_param_types(
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

    /// Emit the HIR items for one monomorphized struct instance: an ordinary
    /// `HirItem::Struct` plus one `HirItem::Impl` per generic impl, with method bodies
    /// lowered under the impl's concrete type-parameter substitution.
    fn emit_mono_struct(&mut self, ms: &crate::MonoStruct) -> Result<(), LoweringError> {
        let template = match self.generic_structs.get(&ms.base) {
            Some(t) => t.clone(),
            None => {
                return Err(LoweringError::UnresolvedType {
                    name: ms.base.clone(),
                })
            }
        };

        let saved_ty = std::mem::replace(&mut self.type_subst, ms.subst.clone());
        let saved_c = std::mem::replace(&mut self.const_subst, ms.const_subst.clone());
        let mut hir_fields = Vec::with_capacity(template.fields.len());
        for field in &template.fields {
            hir_fields.push(HirField {
                name: field.name.name.clone(),
                ty: self.resolve_type(&field.ty)?,
                span: field.span,
            });
        }
        self.type_subst = saved_ty;
        self.const_subst = saved_c;
        self.mono_items.push(HirItem::Struct(HirStruct {
            name: ms.mangled.clone(),
            fields: hir_fields,
            span: template.span,
        }));

        let impls = self
            .generic_impls
            .get(&ms.base)
            .cloned()
            .unwrap_or_default();
        for imp in &impls {
            let impl_subst = self.build_impl_subst(imp, &template.generics, &ms.subst);
            let mut methods = Vec::new();
            for method in &imp.methods {
                if matches!(method.self_param, Some(SelfParam::Owned)) {
                    continue;
                }
                let const_types = self.const_param_types(&template.generics)?;
                let saved_ty = std::mem::replace(&mut self.type_subst, impl_subst.clone());
                let saved_c = std::mem::replace(&mut self.const_subst, ms.const_subst.clone());
                let saved_ct = std::mem::replace(&mut self.const_types, const_types);
                let lowered = self.lower_method(&ms.mangled, method);
                self.type_subst = saved_ty;
                self.const_subst = saved_c;
                self.const_types = saved_ct;
                methods.push(lowered?);
            }
            self.mono_items.push(HirItem::Impl(HirImpl {
                type_name: ms.mangled.clone(),
                trait_name: imp.trait_name.as_ref().map(|t| t.name.clone()),
                methods,
                span: imp.span,
            }));
        }
        Ok(())
    }

    /// Resolve an enum's variants and payload field types into the lowering table
    /// Mirrors the checker's registration; payload-type Copy/scalar
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

    /// Record a trait's methods in declaration order. That order is the vtable
    /// slot order every implementor shares, and the signatures let a call through a
    /// `&dyn Trait` receiver be typed without naming a concrete implementor.
    fn register_trait(&mut self, def: &ast_types::TraitDef) -> Result<(), LoweringError> {
        let mut methods = Vec::with_capacity(def.methods.len());
        for method in &def.methods {
            let mut params = Vec::with_capacity(method.params.len());
            for param in &method.params {
                params.push(self.resolve_type(&param.ty)?);
            }
            let ret = match &method.return_type {
                Some(t) => self.resolve_type(t)?,
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

    fn register_impl(&mut self, def: &ImplDef) -> Result<(), LoweringError> {
        let struct_name = def.type_name.name.clone();
        for method in &def.methods {
            // An owned `self` on a `Copy` receiver is valid for operator-trait methods
            // The checker already rejected it on any non-`Copy` type, so every
            // owned-`self` method reaching lowering is sound and is registered normally.
            let mangled = format!("{}__{}", struct_name, method.name.name);
            let (params, ret) = self.method_signature(&struct_name, method)?;
            self.functions.insert(mangled.clone(), (params, ret));
            self.impl_methods
                .entry(struct_name.clone())
                .or_default()
                .insert(method.name.name.clone(), mangled);
        }
        self.register_operator_impl(def, &struct_name)?;
        Ok(())
    }

    /// Record the operator dispatch for an operator-trait impl, mirroring the
    /// checker so this slice resolves operators on user types independently.
    fn register_operator_impl(
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

    /// The full signature of a method: the implicit `self` (the struct type) leads
    /// the parameter list for an instance method, then the declared parameters.
    fn method_signature(
        &mut self,
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
        Ok(())
    }

    /// The resolved return type of a function, resolving return-position `impl Trait`
    /// to the concrete type the body constructs.
    ///
    /// `impl Trait` in return position is static dispatch — exactly one concrete type
    /// leaves the function — so it is transparent here, exactly as the checker resolved
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
    fn shallow_result_type(&mut self, expr: &ast_types::Expr) -> Option<HirType> {
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
                // A generic template is not lowered directly; only its concrete
                // instantiations, discovered at call sites, reach the HIR.
                Item::Function(func) if !func.generics.is_empty() => {}
                Item::Function(func) => {
                    hir_items.push(HirItem::Function(self.lower_function(func)?))
                }
                // Generic struct / impl templates are likewise never lowered directly;
                // each concrete instance is emitted from the monomorphization worklist.
                Item::Struct(def) if !def.generics.is_empty() => {}
                Item::Impl(def) if !def.generics.is_empty() || !def.type_args.is_empty() => {}
                Item::Struct(def) => hir_items.push(HirItem::Struct(self.lower_struct(def)?)),
                Item::Enum(def) => hir_items.push(HirItem::Enum(self.lower_enum(def)?)),
                Item::Impl(def) => hir_items.push(HirItem::Impl(self.lower_impl(def)?)),
                Item::Const(def) => hir_items.push(HirItem::Const(self.lower_const(def)?)),
                // A newtype is transparent at runtime and produces no HIR item; it
                // survives only as the `HirType::Newtype` its annotations resolve to.
                Item::Newtype(_) => {}
                // A trait emits no code of its own: each `impl Trait for Type`
                // lowers via the ordinary impl path above, with any omitted default
                // method already injected by the parser. The item carries only the
                // declaration-ordered method list backends need to lay out vtables for
                // dynamic dispatch.
                Item::Trait(def) => hir_items.push(HirItem::Trait(neuro_hir::HirTrait {
                    name: def.name.name.clone(),
                    methods: def.methods.iter().map(|m| m.name.name.clone()).collect(),
                    span: def.span,
                })),
            }
        }

        // Drain the monomorphization worklists: lowering the ordinary items above (and
        // each instance below) enqueues every generic function and struct instantiation
        // it references, so this runs until the transitive closure is emitted.
        // Struct instances are drained first because emitting their method bodies can in
        // turn enqueue generic-function instances.
        loop {
            if let Some(ms) = self.mono_struct_pending.pop() {
                self.emit_mono_struct(&ms)?;
                continue;
            }
            if let Some(instance) = self.mono_pending.pop() {
                let hir_fn = self.lower_mono_instance(&instance)?;
                self.mono_items.push(HirItem::Function(hir_fn));
                continue;
            }
            break;
        }
        hir_items.append(&mut self.mono_items);
        // Lifted closures are appended last; their bodies are self-contained and the
        // backend pre-declares every function signature before emitting any body, so
        // position among the items does not matter.
        hir_items.append(&mut self.closure_items);

        Ok(HirProgram { items: hir_items })
    }

    /// Lower one monomorphized instance of a generic template: substitute its type
    /// parameters (via [`Lowerer::type_subst`]) and emit a concrete function named by
    /// the instance's mangled name.
    fn lower_mono_instance(
        &mut self,
        instance: &MonoInstance,
    ) -> Result<HirFunction, LoweringError> {
        let template = match self.generic_templates.get(&instance.fn_name) {
            Some(t) => t.clone(),
            None => {
                return Err(LoweringError::UnresolvedCall {
                    target: instance.fn_name.clone(),
                })
            }
        };

        let const_types = self.const_param_types(&template.generics)?;
        let saved_ty = std::mem::replace(&mut self.type_subst, instance.subst.clone());
        let saved_c = std::mem::replace(&mut self.const_subst, instance.const_subst.clone());
        let saved_ct = std::mem::replace(&mut self.const_types, const_types);

        let mut params = Vec::with_capacity(template.params.len());
        for param in &template.params {
            params.push(HirParam {
                name: param.name.name.clone(),
                ty: self.resolve_type(&param.ty)?,
                span: param.span,
            });
        }
        let return_type = match &template.return_type {
            Some(t) => self.resolve_type(t)?,
            None => HirType::Void,
        };

        self.push_scope();
        for param in &params {
            self.define(param.name.clone(), param.ty.clone());
        }
        let body = self.lower_body(&template.body, &return_type)?;
        self.pop_scope();

        self.type_subst = saved_ty;
        self.const_subst = saved_c;
        self.const_types = saved_ct;

        Ok(HirFunction {
            name: instance.mangled.clone(),
            params,
            return_type,
            body,
            span: template.span,
        })
    }

    fn lower_struct(&mut self, def: &StructDef) -> Result<HirStruct, LoweringError> {
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

    fn lower_enum(&mut self, def: &EnumDef) -> Result<HirEnum, LoweringError> {
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
        let return_type = self.declared_return_type(&func.return_type, &func.body)?;

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
            // Owned `self` on a `Copy` receiver is a valid operator-trait method;
            // the checker rejected it on any non-`Copy` type, so it is lowered like any
            // other method (an owned `Copy` receiver is ABI-identical to `&self`).
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
    /// the contextual hint the checker applies; every other statement lowers
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

/// The expression a body evaluates to: its trailing expression, or the operand of a
/// trailing `return`. Used for return-position `impl Trait` resolution.
fn body_result_expr(body: &[ast_types::Stmt]) -> Option<&ast_types::Expr> {
    match body.last()? {
        ast_types::Stmt::Expr(expr) => Some(expr),
        ast_types::Stmt::Return { value, .. } => value.as_ref(),
        _ => None,
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

/// Split a monomorphized instance's positional arguments into a type substitution and a
/// const substitution, keyed by the template's generic parameter names in order.
fn split_mono_args(
    generics: &[ast_types::GenericParam],
    args: &[crate::MonoArg],
) -> (
    std::collections::HashMap<String, HirType>,
    std::collections::HashMap<String, u64>,
) {
    let mut type_subst = std::collections::HashMap::new();
    let mut const_subst = std::collections::HashMap::new();
    for (gp, arg) in generics.iter().zip(args) {
        match arg {
            crate::MonoArg::Type(t) => {
                type_subst.insert(gp.name.name.clone(), t.clone());
            }
            crate::MonoArg::Const(v) => {
                const_subst.insert(gp.name.name.clone(), *v);
            }
        }
    }
    (type_subst, const_subst)
}
