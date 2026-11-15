//! Return-position `impl Trait`: the concrete type a function's body produces, found by
//! lowering the body on a copy of the `Lowerer`, as the checker finds it by checking
//! the body. Duplicated from the checker, which owns its own type tables.

use ast_types::{FunctionDef, Item, Stmt};
use neuro_hir::HirType;

use crate::{Lowerer, LoweringError, MonoArg};

pub(super) fn returns_impl_trait(func: &FunctionDef) -> bool {
    matches!(func.return_type, Some(ast_types::Type::ImplTrait { .. }))
}

impl Lowerer {
    /// Register every non-generic `impl Trait` function under the type its body produces.
    /// One still unregistered is an unresolved call to any body that names it, so a body
    /// calling one waits for a later round, in the order the checker resolved them.
    pub(super) fn resolve_impl_returns(&mut self, items: &[Item]) -> Result<(), LoweringError> {
        let mut pending = Vec::new();
        for item in items {
            if let Item::Function(func) = item
                && func.generics.is_empty()
                && returns_impl_trait(func)
            {
                let params = func
                    .params
                    .iter()
                    .map(|param| self.resolve_type(&param.ty))
                    .collect::<Result<Vec<_>, _>>()?;
                pending.push((func, params));
            }
        }
        // ponytail: quadratic in a chain of `impl Trait` functions calling each other, as
        // in the checker; a call graph orders them in one pass.
        loop {
            let before = pending.len();
            let mut unresolved = Vec::with_capacity(before);
            for (func, params) in pending {
                match self.probe_impl_return(func, &params) {
                    Ok(ret) => {
                        self.functions.insert(func.name.name.clone(), (params, ret));
                    }
                    Err(_) => unresolved.push((func, params)),
                }
            }
            pending = unresolved;
            if pending.is_empty() || pending.len() == before {
                break;
            }
        }
        match pending.first() {
            Some((func, params)) => self.probe_impl_return(func, params).map(drop),
            None => Ok(()),
        }
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
        // A trailing `return` ends the body with its operand, as a tail expression does.
        let (_, ty) = match func.body.split_last() {
            Some((
                Stmt::Return {
                    value: Some(value), ..
                },
                init,
            )) => {
                let mut body = init.to_vec();
                body.push(Stmt::Expr(value.clone()));
                probe.lower_inferred_body(&body)?
            }
            _ => probe.lower_inferred_body(&func.body)?,
        };
        self.adopt_instances(&probe, &ty)?;
        Ok(ty)
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
