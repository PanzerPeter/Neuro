//! Monomorphization: a generic struct or enum instantiated per type-argument list, its
//! impl methods registered for the instance, and the instance items emitted.

use ast_types::ImplDef;
use neuro_hir::{HirField, HirFunction, HirImpl, HirItem, HirParam, HirStruct, HirType};

use crate::{Lowerer, LoweringError, MonoInstance};

/// Split a monomorphized instance's positional arguments into a type substitution and a
/// const substitution, keyed by the template's generic parameter names in order.
pub(super) fn split_mono_args(
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

impl Lowerer {
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
                });
            }
        };
        let mangled = crate::mangle_struct_instance(base, args);
        if !self.struct_instances.contains_key(&mangled) {
            let depth = self.next_instance_depth(base)?;
            self.struct_instances
                .insert(mangled.clone(), (base.to_string(), args.to_vec()));
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
            if self.partial_eq_structs.contains(base) {
                self.partial_eq_structs.insert(mangled.clone());
            }

            self.register_instance_methods(base, &mangled, &subst, &const_subst)?;
            self.mono_struct_pending.push(crate::MonoStruct {
                depth,
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
    pub(super) fn build_impl_subst(
        &self,
        imp: &ImplDef,
        base_generics: &[ast_types::GenericParam],
        subst: &std::collections::HashMap<String, HirType>,
    ) -> std::collections::HashMap<String, HirType> {
        let mut result = std::collections::HashMap::new();
        for (ta, gp) in imp.type_args.iter().zip(base_generics) {
            if let ast_types::Type::Named(id) = ta
                && imp.generics.iter().any(|g| g.name.name == id.name)
                && let Some(concrete) = subst.get(&gp.name.name)
            {
                result.insert(id.name.clone(), concrete.clone());
            }
        }
        result
    }

    /// Register the signature of every method of each generic `impl` of `base` for the
    /// concrete instance `mangled`, so calls on the instance resolve.
    pub(super) fn register_instance_methods(
        &mut self,
        base: &str,
        mangled: &str,
        subst: &std::collections::HashMap<String, HirType>,
        const_subst: &std::collections::HashMap<String, u64>,
    ) -> Result<(), LoweringError> {
        let impls = self.generic_impls.get(base).cloned().unwrap_or_default();
        let base_generics = self.template_generics(base);
        let self_ty = self.impl_target_type(mangled)?;
        for imp in &impls {
            let impl_subst = self.build_impl_subst(imp, &base_generics, subst);
            for method in &imp.methods {
                let inst_key = format!("{}__{}", mangled, method.name.name);
                if self.functions.contains_key(&inst_key) {
                    continue;
                }
                let saved_ty = std::mem::replace(&mut self.type_subst, impl_subst.clone());
                let saved_c = std::mem::replace(&mut self.const_subst, const_subst.clone());
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
                self.type_subst = saved_ty;
                self.const_subst = saved_c;
                self.functions.insert(inst_key.clone(), (params, ret));
                self.impl_methods
                    .entry(mangled.to_string())
                    .or_default()
                    .insert(method.name.name.clone(), inst_key);
            }
            // The instance implements whatever the template's impl did. Mirrors the
            // checker; without it a `for` head over a generic iterator adapter would
            // find no protocol on the instance the head actually has.
            if let Some(trait_name) = &imp.trait_name {
                self.trait_impls
                    .insert((trait_name.name.clone(), mangled.to_string()));
            }
        }
        Ok(())
    }

    /// Give every enum instance built before the generic impls were recorded (a field
    /// or payload naming `Tree<i32>` resolves in the declaration pass) its methods.
    pub(super) fn register_existing_enum_instance_methods(&mut self) -> Result<(), LoweringError> {
        let existing: Vec<(String, String)> = self
            .enum_instance_base
            .iter()
            .filter(|(_, base)| self.generic_impls.contains_key(*base))
            .map(|(mangled, base)| (mangled.clone(), base.clone()))
            .collect();
        for (mangled, base) in existing {
            let args = self
                .enum_instance_args
                .get(&mangled)
                .cloned()
                .unwrap_or_default();
            let (subst, const_subst) = split_mono_args(&self.template_generics(&base), &args);
            self.register_instance_methods(&base, &mangled, &subst, &const_subst)?;
        }
        Ok(())
    }

    /// Emit the HIR items for one monomorphized struct instance: an ordinary
    /// `HirItem::Struct` plus one `HirItem::Impl` per generic impl, with method bodies
    /// lowered under the impl's concrete type-parameter substitution.
    pub(super) fn emit_mono_struct(&mut self, ms: &crate::MonoStruct) -> Result<(), LoweringError> {
        let template = match self.generic_structs.get(&ms.base) {
            Some(t) => t.clone(),
            None => {
                return Err(LoweringError::UnresolvedType {
                    name: ms.base.clone(),
                });
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
            written_name: ms.base.clone(),
            fields: hir_fields,
            span: template.span,
        }));

        self.emit_instance_impls(
            &ms.base,
            &ms.mangled,
            &template.generics,
            &ms.subst,
            &ms.const_subst,
        )
    }

    /// Emit one `HirItem::Impl` per generic impl of `base` for the instance `mangled`,
    /// with method bodies lowered under the impl's concrete type-parameter substitution.
    pub(super) fn emit_instance_impls(
        &mut self,
        base: &str,
        mangled: &str,
        generics: &[ast_types::GenericParam],
        subst: &std::collections::HashMap<String, HirType>,
        const_subst: &std::collections::HashMap<String, u64>,
    ) -> Result<(), LoweringError> {
        let impls = self.generic_impls.get(base).cloned().unwrap_or_default();
        let self_type = self.impl_target_type(mangled)?;
        for imp in &impls {
            let impl_subst = self.build_impl_subst(imp, generics, subst);
            let mut methods = Vec::new();
            for method in &imp.methods {
                let const_types = self.const_param_types(generics)?;
                let saved_ty = std::mem::replace(&mut self.type_subst, impl_subst.clone());
                let saved_c = std::mem::replace(&mut self.const_subst, const_subst.clone());
                let saved_ct = std::mem::replace(&mut self.const_types, const_types);
                let lowered = self.lower_method(&self_type, method);
                self.type_subst = saved_ty;
                self.const_subst = saved_c;
                self.const_types = saved_ct;
                methods.push(lowered?);
            }
            self.mono_items.push(HirItem::Impl(HirImpl {
                type_name: mangled.to_string(),
                self_type: self_type.clone(),
                trait_name: imp.trait_name.as_ref().map(|t| t.name.clone()),
                methods,
                span: imp.span,
            }));
        }
        Ok(())
    }

    /// Materialize a monomorphized generic-enum instance: register its concrete
    /// variants, queue its HIR item for emission, and return the mangled instance name.
    /// Idempotent per instance.
    pub(crate) fn instantiate_generic_enum(
        &mut self,
        base: &str,
        args: &[crate::MonoArg],
    ) -> Result<String, LoweringError> {
        let template = match self.generic_enums.get(base) {
            Some(t) => t.clone(),
            None => {
                return Err(LoweringError::UnresolvedType {
                    name: base.to_string(),
                });
            }
        };
        let mangled = crate::mangle_struct_instance(base, args);
        if !self.enums.contains_key(&mangled) {
            let depth = self.next_instance_depth(base)?;
            let (subst, const_subst) = split_mono_args(&template.generics, args);
            let saved_ty = std::mem::replace(&mut self.type_subst, subst.clone());
            let saved_c = std::mem::replace(&mut self.const_subst, const_subst.clone());
            let variants = self.resolve_variants(&template);
            self.type_subst = saved_ty;
            self.const_subst = saved_c;
            self.enums.insert(mangled.clone(), variants?);
            self.enum_instance_base
                .insert(mangled.clone(), base.to_string());
            self.enum_instance_args
                .insert(mangled.clone(), args.to_vec());
            self.register_instance_methods(base, &mangled, &subst, &const_subst)?;
            self.mono_enum_pending.push(crate::MonoEnum {
                depth,
                base: base.to_string(),
                mangled: mangled.clone(),
                subst,
                const_subst,
            });
        }
        Ok(mangled)
    }

    /// The concrete instance a construction or pattern written with a generic enum's
    /// base name refers to, taken from the surrounding expected type.
    pub(crate) fn enum_instance_from_expected(
        &self,
        base: &str,
        expected: Option<&HirType>,
    ) -> Option<String> {
        let Some(HirType::Enum(name)) = expected else {
            return None;
        };
        (self.enum_instance_base.get(name).map(|s| s.as_str()) == Some(base)).then(|| name.clone())
    }

    /// Emit the HIR item for one monomorphized enum instance: an ordinary
    /// `HirItem::Enum` whose payload types are fully concrete.
    pub(super) fn emit_mono_enum(&mut self, me: &crate::MonoEnum) -> Result<(), LoweringError> {
        let template = match self.generic_enums.get(&me.base) {
            Some(t) => t.clone(),
            None => {
                return Err(LoweringError::UnresolvedType {
                    name: me.base.clone(),
                });
            }
        };
        let saved_ty = std::mem::replace(&mut self.type_subst, me.subst.clone());
        let saved_c = std::mem::replace(&mut self.const_subst, me.const_subst.clone());
        let lowered = self.lower_enum(&template);
        self.type_subst = saved_ty;
        self.const_subst = saved_c;
        let mut lowered = lowered?;
        lowered.name = me.mangled.clone();
        self.mono_items.push(HirItem::Enum(lowered));
        self.emit_instance_impls(
            &me.base,
            &me.mangled,
            &template.generics,
            &me.subst,
            &me.const_subst,
        )
    }

    /// Lower one monomorphized instance of a generic template: substitute its type
    /// parameters (via [`Lowerer::type_subst`]) and emit a concrete function named by
    /// the instance's mangled name.
    pub(super) fn lower_mono_instance(
        &mut self,
        instance: &MonoInstance,
    ) -> Result<HirFunction, LoweringError> {
        let template = match self.generic_templates.get(&instance.fn_name) {
            Some(t) => t.clone(),
            None => {
                return Err(LoweringError::UnresolvedCall {
                    target: instance.fn_name.clone(),
                });
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

        let target = super::target_of(&template.attributes);
        self.push_scope();
        for param in &params {
            self.define(param.name.clone(), param.ty.clone());
        }
        let body = self.lower_function_body(&template.body, &return_type, target)?;
        self.pop_scope();

        self.type_subst = saved_ty;
        self.const_subst = saved_c;
        self.const_types = saved_ct;

        Ok(HirFunction {
            name: instance.mangled.clone(),
            params,
            return_type,
            body,
            target,
            span: template.span,
        })
    }
}
