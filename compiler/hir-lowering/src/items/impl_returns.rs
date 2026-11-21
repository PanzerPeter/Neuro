//! Return-position `impl Trait`: the concrete type a function's or an inherent method's
//! body produces, found by lowering the body on a copy of the `Lowerer`, as the checker
//! finds it by checking the body. Duplicated from the checker, which owns its own type
//! tables.

use ast_types::{FunctionDef, ImplDef, Item, MethodDef, Stmt};
use neuro_hir::HirType;

use crate::{Lowerer, LoweringError, MonoArg};

pub(super) fn returns_impl_trait(func: &FunctionDef) -> bool {
    matches!(func.return_type, Some(ast_types::Type::ImplTrait { .. }))
}

/// Whether `method` returns an `impl Trait` its body decides: the checker's rule, a method
/// of an inherent, non-generic `impl` with no type parameters of its own.
pub(super) fn method_returns_impl_trait(def: &ImplDef, method: &MethodDef) -> bool {
    matches!(method.return_type, Some(ast_types::Type::ImplTrait { .. }))
        && def.trait_name.is_none()
        && def.generics.is_empty()
        && def.type_args.is_empty()
        && method.generics.is_empty()
}

/// A definition whose `impl Trait` return is being found, with its resolved parameters.
enum Pending<'a> {
    Function(&'a FunctionDef, Vec<HirType>),
    /// The method, its `impl`, and `self`'s type when it takes one.
    Method(&'a ImplDef, &'a MethodDef, Option<HirType>, Vec<HirType>),
}

impl Lowerer {
    /// Register every non-generic `impl Trait` function under the type its body produces.
    /// One still unregistered is an unresolved call to any body that names it, so a body
    /// calling one waits for a later round, in the order the checker resolved them.
    pub(super) fn resolve_impl_returns(&mut self, items: &[Item]) -> Result<(), LoweringError> {
        let mut pending = Vec::new();
        for item in items {
            match item {
                Item::Function(func) if func.generics.is_empty() && returns_impl_trait(func) => {
                    let params = func
                        .params
                        .iter()
                        .map(|param| self.resolve_type(&param.ty))
                        .collect::<Result<Vec<_>, _>>()?;
                    pending.push(Pending::Function(func, params));
                }
                Item::Impl(def) => {
                    for method in &def.methods {
                        if !method_returns_impl_trait(def, method) {
                            continue;
                        }
                        let saved = self.enter_impl_assoc(def)?;
                        let self_ty = match method.self_param {
                            Some(_) => Some(self.impl_target_type(&def.type_name.name)?),
                            None => None,
                        };
                        let params = method
                            .params
                            .iter()
                            .map(|param| self.resolve_type(&param.ty))
                            .collect::<Result<Vec<_>, _>>();
                        self.type_subst = saved;
                        pending.push(Pending::Method(def, method, self_ty, params?));
                    }
                }
                _ => {}
            }
        }
        // ponytail: quadratic in a chain of `impl Trait` functions calling each other, as
        // in the checker; a call graph orders them in one pass.
        loop {
            let before = pending.len();
            let mut unresolved = Vec::with_capacity(before);
            for site in pending {
                match self.probe_pending(&site) {
                    Ok(ret) => self.settle_pending(site, ret),
                    Err(_) => unresolved.push(site),
                }
            }
            pending = unresolved;
            if pending.is_empty() || pending.len() == before {
                break;
            }
        }
        match pending.first() {
            Some(site) => self.probe_pending(site).map(drop),
            None => Ok(()),
        }
    }

    fn probe_pending(&mut self, site: &Pending) -> Result<HirType, LoweringError> {
        match site {
            Pending::Function(func, params) => self.probe_impl_return(func, params),
            Pending::Method(def, method, self_ty, params) => {
                let saved = self.enter_impl_assoc(def)?;
                let probed = self.probe_method_return(def, method, self_ty.as_ref(), params);
                self.type_subst = saved;
                probed
            }
        }
    }

    /// Register a resolved definition under the type its body produced. A method joins
    /// its type's method table only now, so a body calling it before then fails to
    /// resolve the call and waits for a later round.
    fn settle_pending(&mut self, site: Pending, ret: HirType) {
        match site {
            Pending::Function(func, params) => {
                self.functions.insert(func.name.name.clone(), (params, ret));
            }
            Pending::Method(def, method, self_ty, params) => {
                let type_name = &def.type_name.name;
                let mangled = format!("{}__{}", type_name, method.name.name);
                let params = self_ty.into_iter().chain(params).collect();
                self.functions.insert(mangled.clone(), (params, ret));
                self.impl_methods
                    .entry(type_name.clone())
                    .or_default()
                    .insert(method.name.name.clone(), mangled);
            }
        }
    }

    /// The type `method`'s body ends with, as [`Self::probe_impl_return`] finds a
    /// function's.
    fn probe_method_return(
        &mut self,
        def: &ImplDef,
        method: &MethodDef,
        self_ty: Option<&HirType>,
        param_types: &[HirType],
    ) -> Result<HirType, LoweringError> {
        let key = format!("{}__{}", def.type_name.name, method.name.name);
        if self.probing_returns.contains(&key) {
            return Err(LoweringError::UnresolvedType {
                name: format!("impl return of `{key}`"),
            });
        }
        let mut probe = self.clone();
        probe.probing_returns.insert(key);
        probe.push_scope();
        if let Some(self_ty) = self_ty {
            probe.define("self".to_string(), self_ty.clone());
        }
        for (param, ty) in method.params.iter().zip(param_types) {
            probe.define(param.name.name.clone(), ty.clone());
        }
        let (_, ty) = probe.lower_tail_body(&method.body)?;
        self.adopt_instances(&probe, &ty)?;
        Ok(ty)
    }

    /// The return type of a generic template at the instance the current substitution
    /// names: an `impl Trait` return is what the instance's body produces.
    pub(crate) fn template_return_type(
        &mut self,
        template: &FunctionDef,
        params: &[HirType],
    ) -> Result<HirType, LoweringError> {
        match &template.return_type {
            Some(ast_types::Type::ImplTrait { .. }) => self.probe_impl_return(template, params),
            Some(t) => self.resolve_type(t),
            None => Ok(HirType::Void),
        }
    }

    /// The type `func`'s body ends with, lowered on a copy so nothing it records is kept.
    /// A body that reaches its own probe again depends on its own result, which the
    /// checker refuses; the guard turns that into an error instead of endless recursion.
    fn probe_impl_return(
        &mut self,
        func: &FunctionDef,
        param_types: &[HirType],
    ) -> Result<HirType, LoweringError> {
        if self.probing_returns.contains(&func.name.name) {
            return Err(LoweringError::UnresolvedType {
                name: format!("impl return of `{}`", func.name.name),
            });
        }
        let mut probe = self.clone();
        probe.probing_returns.insert(func.name.name.clone());
        probe.push_scope();
        for (param, ty) in func.params.iter().zip(param_types) {
            probe.define(param.name.name.clone(), ty.clone());
        }
        let (_, ty) = probe.lower_tail_body(&func.body)?;
        self.adopt_instances(&probe, &ty)?;
        Ok(ty)
    }

    /// Lower `body` for the type it ends with. A trailing `return` ends it with its
    /// operand, as a tail expression does.
    fn lower_tail_body(
        &mut self,
        body: &[Stmt],
    ) -> Result<(Vec<neuro_hir::HirStmt>, HirType), LoweringError> {
        match body.split_last() {
            Some((
                Stmt::Return {
                    value: Some(value), ..
                },
                init,
            )) => {
                let mut body = init.to_vec();
                body.push(Stmt::Expr(value.clone()));
                self.lower_inferred_body(&body)
            }
            _ => self.lower_inferred_body(body),
        }
    }

    /// Make here the generic instances `ty` names that only `probe` made, so a body
    /// lowered later can call their methods.
    fn adopt_instances(&mut self, probe: &Lowerer, ty: &HirType) -> Result<(), LoweringError> {
        match ty {
            HirType::Struct(name) | HirType::Enum(name) => {
                if self.instance_args(name).is_some() {
                    return Ok(());
                }
                let Some((base, args)) = probe.instance_args(name) else {
                    return Ok(());
                };
                for arg in args {
                    if let MonoArg::Type(arg) = arg {
                        self.adopt_instances(probe, arg)?;
                    }
                }
                if matches!(ty, HirType::Struct(_)) {
                    self.instantiate_generic_struct(base, args)?;
                } else {
                    self.instantiate_generic_enum(base, args)?;
                }
            }
            HirType::Reference { inner, .. }
            | HirType::Slice(inner)
            | HirType::Array { element: inner, .. }
            | HirType::Newtype { inner, .. } => self.adopt_instances(probe, inner)?,
            HirType::Tuple(elements) | HirType::Collection { args: elements, .. } => {
                for element in elements {
                    self.adopt_instances(probe, element)?;
                }
            }
            HirType::Function { params, ret } => {
                for param in params {
                    self.adopt_instances(probe, param)?;
                }
                self.adopt_instances(probe, ret)?;
            }
            _ => {}
        }
        Ok(())
    }
}
