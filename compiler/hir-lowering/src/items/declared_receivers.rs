//! The `.clone()` receivers a generic body declares in a type parameter.
//!
//! A method call reads through every borrow on its receiver, but an instance holds only
//! concrete types: where `T` is `&S`, `v: T` and `s: &S` look alike, and `v.clone()` has to
//! copy the reference while `s.clone()` reaches `S`. So the template of such an instance is
//! first lowered on a throwaway [`Lowerer`] with `T` as an opaque struct, and every receiver
//! that reaches one is declared in `T`, whichever inference carried it there. A reference
//! meets `Copy` and `Clone` and nothing else, so no other method can reach `T` here.

use std::collections::HashMap;

use ast_types::{GenericParam, ImplDef};
use neuro_hir::{HirExpr, HirType};
use shared_types::Span;

use super::register::{CLONE_TRAIT, COPY_TRAIT};
use crate::{Lowerer, LoweringError, MonoArg};

/// Starts the name of the struct standing in for a type parameter. `$` cannot appear in a
/// source identifier, so no program can name one.
const OPAQUE_PARAM: &str = "$param$";

type Subst = HashMap<String, HirType>;

impl Lowerer {
    /// The declared receivers of the body `lower` lowers: it runs on a copy of this lowerer
    /// with `subst` rewritten so each of `params` that can reach `.clone()` on a reference
    /// is opaque. Empty, with `lower` never run, when no parameter qualifies.
    // ponytail: clones the whole lowerer per such instance, O(program) each. Fine while a
    // reference-bound `T: Clone` is rare; snapshot only what a body touches if it shows up.
    pub(super) fn declared_receivers(
        &self,
        params: &[&GenericParam],
        subst: &Subst,
        lower: impl FnOnce(&mut Lowerer, Subst) -> Result<(), LoweringError>,
    ) -> Result<HashMap<Span, u32>, LoweringError> {
        let opaque: Vec<_> = params
            .iter()
            .filter(|param| self.clones_a_reference(param, subst))
            .collect();
        if opaque.is_empty() {
            return Ok(HashMap::new());
        }
        let mut pass = self.clone();
        let mut subst = subst.clone();
        for param in opaque {
            let marker = format!("{OPAQUE_PARAM}{}", param.name.name);
            pass.structs.insert(marker.clone(), Vec::new());
            pass.clone_structs.insert(marker.clone());
            pass.copy_structs.insert(marker.clone());
            subst.insert(param.name.name.clone(), HirType::Struct(marker));
        }
        lower(&mut pass, subst)?;
        Ok(pass.param_receivers)
    }

    /// Whether `param` is bound to a `&T` here and has `.clone()` through a bound.
    fn clones_a_reference(&self, param: &GenericParam, subst: &Subst) -> bool {
        matches!(
            subst.get(&param.name.name),
            Some(HirType::Reference { mutable: false, .. })
        ) && param.bounds.iter().any(|bound| {
            self.trait_closure(&bound.trait_name.name)
                .iter()
                .any(|name| name == CLONE_TRAIT || name == COPY_TRAIT)
        })
    }

    /// The impl that declares `method` on the generic instance `instance`, if one does.
    pub(super) fn impl_declaring(&self, instance: &str, method: &str) -> Option<ImplDef> {
        let (base, _) = self.instance_args(instance)?;
        self.generic_impls
            .get(base)?
            .iter()
            .find(|imp| imp.methods.iter().any(|m| m.name.name == method))
            .cloned()
    }

    /// `imp`'s target as the instance `instance` with its parameters taken from `subst`, so
    /// a field declared in an opaque parameter reads as that parameter.
    pub(super) fn opaque_self_type(
        &mut self,
        imp: &ImplDef,
        instance: &str,
        subst: &Subst,
    ) -> Result<HirType, LoweringError> {
        let Some((base, args)) = self.instance_args(instance) else {
            return self.impl_target_type(instance);
        };
        let base = base.to_string();
        let mut args = args.to_vec();
        for (arg, written) in args.iter_mut().zip(&imp.type_args) {
            if let ast_types::Type::Named(id) = written
                && let Some(ty) = subst.get(&id.name)
            {
                *arg = MonoArg::Type(ty.clone());
            }
        }
        let mangled = if self.generic_enums.contains_key(&base) {
            self.instantiate_generic_enum(&base, &args)?
        } else {
            self.instantiate_generic_struct(&base, &args)?
        };
        self.impl_target_type(&mangled)
    }

    /// `object.clone()` when the call's receiver is declared in a reference-bound parameter:
    /// the reference itself, read out of the borrows the declaration wraps it in. A receiver
    /// reaching an opaque parameter is recorded instead, which is the template pass's job.
    pub(crate) fn clone_of_declared_param(
        &mut self,
        object: &HirExpr,
        span: Span,
    ) -> Option<HirExpr> {
        if let Some(borrows) = borrows_above_opaque(&object.ty) {
            self.param_receivers.insert(span, borrows);
            return None;
        }
        let borrows = *self.param_receivers.get(&span)?;
        Some((0..borrows).fold(object.clone(), |read, _| {
            crate::derivable::read_through(read)
        }))
    }
}

/// How many borrows wrap an opaque parameter in `ty`, if `ty` is one under borrows alone.
fn borrows_above_opaque(ty: &HirType) -> Option<u32> {
    match ty {
        HirType::Struct(name) if name.starts_with(OPAQUE_PARAM) => Some(0),
        HirType::Reference { inner, .. } => borrows_above_opaque(inner).map(|depth| depth + 1),
        _ => None,
    }
}
