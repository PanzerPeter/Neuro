//! Return-position `impl Trait`: the one concrete type a function's or an inherent
//! method's body produces, read by checking the body once every other signature is
//! registered.
//!
//! One of the declaration-kind modules under `declarations`; each adds methods
//! to the same `impl TypeChecker` block.

use super::impls::returns_impl_trait;
use crate::errors::TypeError;
use crate::type_checkers::{GenericFnSig, TypeChecker};
use crate::types::Type;
use ast_types::{FunctionDef, ImplDef, Item, MethodDef, Stmt};
use shared_types::{Identifier, Span};
use std::collections::HashSet;

/// A definition whose `impl Trait` return its body decides.
#[derive(Clone, Copy)]
enum Site<'a> {
    Function(&'a FunctionDef),
    /// A method of an inherent, non-generic `impl`, with none of its own parameters.
    Method(&'a ImplDef, &'a MethodDef),
}

impl Site<'_> {
    /// The key its signature is registered under in `functions` or `generic_funcs`.
    fn key(&self) -> String {
        match self {
            Site::Function(func) => func.name.name.clone(),
            Site::Method(def, method) => format!("{}__{}", def.type_name.name, method.name.name),
        }
    }

    fn body(&self) -> &[Stmt] {
        match self {
            Site::Function(func) => &func.body,
            Site::Method(_, method) => &method.body,
        }
    }

    fn is_generic(&self) -> bool {
        matches!(self, Site::Function(func) if !func.generics.is_empty())
    }
}

/// A function's registered signature, kept out of the tables while its return type is
/// unresolved.
enum Held {
    Plain(Type),
    Generic(Box<GenericFnSig>),
}

impl Held {
    fn params(&self) -> &[Type] {
        match self {
            Held::Plain(Type::Function { params, .. }) => params,
            Held::Plain(_) => &[],
            Held::Generic(sig) => &sig.params,
        }
    }
}

/// What checking a body for the type it ends with found.
enum Probe {
    Typed(Type),
    /// The body ends in no value.
    Untyped,
    /// The check reported an error.
    Failed,
}

/// The `Assoc = T` bindings an `impl Trait` names.
type AssocBindings = [(Identifier, ast_types::Type)];

/// The `impl Trait` a definition returns: its trait, its `Assoc = T` bindings and span.
fn impl_return<'a>(site: &Site<'a>) -> Option<(&'a Identifier, &'a AssocBindings, Span)> {
    let return_type = match site {
        Site::Function(func) => func.return_type.as_ref(),
        Site::Method(_, method) => method.return_type.as_ref(),
    };
    match return_type {
        Some(ast_types::Type::ImplTrait {
            trait_name,
            assoc_bindings,
            span,
        }) => Some((trait_name, assoc_bindings, *span)),
        _ => None,
    }
}

impl TypeChecker {
    /// Give every return-position `impl Trait` the type its body produces.
    ///
    /// `impl Trait` in return position is static dispatch: one concrete type leaves the
    /// function, so the signature becomes that type and callers see it at no cost.
    /// An unresolved function stays out of the signature tables, so a body that calls
    /// one fails its check and waits for a later round. One that never resolves calls
    /// itself, directly or through another, or ends in no value, and is reported; one
    /// whose body has errors of its own is left to pass 4 to report them.
    pub(crate) fn resolve_impl_returns(&mut self, items: &[Item]) {
        let sites = items.iter().flat_map(|item| match item {
            Item::Function(func) => vec![Site::Function(func)],
            Item::Impl(def) => def
                .methods
                .iter()
                .filter(|method| returns_impl_trait(def, method))
                .map(|method| Site::Method(def, method))
                .collect(),
            _ => Vec::new(),
        });
        let mut pending = Vec::new();
        for site in sites {
            let Some((trait_name, _, span)) = impl_return(&site) else {
                continue;
            };
            if !self.traits.contains_key(&trait_name.name) {
                self.record_error(TypeError::UnknownTrait {
                    trait_name: trait_name.name.clone(),
                    span,
                });
                continue;
            }
            if let Some(held) = self.take_signature(&site) {
                pending.push((site, held));
            }
        }
        // ponytail: each round re-checks every unresolved body, quadratic in a chain of
        // `impl Trait` functions calling each other; ordering them by call graph is one pass.
        loop {
            let before = pending.len();
            let mut unresolved = Vec::with_capacity(before);
            for (site, held) in pending {
                match self.probe_body_type(&site, held.params()) {
                    // A call to a method still pending types as `Unknown` rather than
                    // failing, so the body waits for a later round as one calling a
                    // pending function does.
                    Probe::Typed(Type::Unknown) | Probe::Untyped | Probe::Failed => {
                        unresolved.push((site, held));
                    }
                    Probe::Typed(ty) => self.settle_impl_return(&site, held, ty),
                }
            }
            pending = unresolved;
            if pending.is_empty() || pending.len() == before {
                break;
            }
        }
        // With every leftover callable at `Unknown`, a check that still fails failed on
        // the body's own errors, which pass 4 reports.
        let mut leftovers = Vec::with_capacity(pending.len());
        for (site, held) in pending {
            leftovers.push((site, held.params().to_vec()));
            self.restore_signature(&site, held, Type::Unknown);
        }
        for (site, params) in leftovers {
            if let Probe::Failed = self.probe_body_type(&site, &params) {
                continue;
            }
            if let Some((trait_name, _, span)) = impl_return(&site) {
                self.record_error(TypeError::ImplReturnNotInferable {
                    trait_name: trait_name.name.clone(),
                    span,
                });
            }
        }
    }

    fn take_signature(&mut self, site: &Site) -> Option<Held> {
        if site.is_generic() {
            self.generic_funcs
                .remove(&site.key())
                .map(|sig| Held::Generic(Box::new(sig)))
        } else {
            self.functions.remove(&site.key()).map(Held::Plain)
        }
    }

    fn restore_signature(&mut self, site: &Site, held: Held, ret: Type) {
        match held {
            Held::Plain(Type::Function { params, mode, .. }) => {
                self.functions.insert(
                    site.key(),
                    Type::Function {
                        params,
                        ret: Box::new(ret),
                        mode,
                    },
                );
            }
            Held::Plain(other) => {
                self.functions.insert(site.key(), other);
            }
            Held::Generic(mut sig) => {
                sig.ret = ret;
                self.generic_funcs.insert(site.key(), *sig);
            }
        }
    }

    /// Check a body for the type it ends with. Nothing the check reports is kept: pass 4
    /// checks the body again, against the type found here.
    fn probe_body_type(&mut self, site: &Site, param_types: &[Type]) -> Probe {
        let errors = self.errors.len();
        let warnings = self.warnings.len();
        let structs: HashSet<String> = self.struct_instances.keys().cloned().collect();
        let enums: HashSet<String> = self.enum_instances.keys().cloned().collect();
        let (module, opened) = match site {
            Site::Function(func) => {
                let module = std::mem::replace(&mut self.current_module, func.module);
                self.enter_generic_scope(&func.generics, &func.lifetimes);
                self.enter_function_body(func, param_types, &Type::Unknown);
                (module, None)
            }
            Site::Method(def, method) => {
                let module = std::mem::replace(
                    &mut self.current_module,
                    method.module.unwrap_or(def.module),
                );
                let assoc = self.enter_impl_assoc(def);
                let scope = self.enter_method_body(
                    &def.type_name.name,
                    method,
                    param_types,
                    &Type::Unknown,
                );
                (module, Some((assoc, scope)))
            }
        };
        let ty = site.body().split_last().and_then(|(last, init)| {
            for stmt in init {
                let _ = self.check_stmt(stmt);
            }
            match last {
                Stmt::Expr(expr)
                | Stmt::Return {
                    value: Some(expr), ..
                } => self.check_expr(expr, None),
                Stmt::If {
                    condition,
                    then_block,
                    else_if_blocks,
                    else_block: else_block @ Some(_),
                    span,
                } => self.check_if_expr(
                    condition,
                    then_block,
                    else_if_blocks,
                    else_block,
                    span,
                    None,
                ),
                _ => None,
            }
        });
        match opened {
            None => {
                self.leave_function_body();
                self.exit_generic_scope();
            }
            Some((assoc, scope)) => {
                self.leave_method_body(scope);
                self.self_assoc = assoc;
            }
        }
        self.current_module = module;
        let clean = self.errors.len() == errors;
        self.errors.truncate(errors);
        self.warnings.truncate(warnings);
        if !clean {
            // An instance reports its errors once, when it is made, so one made here
            // would hide them from pass 4. Forgetting it has pass 4 make it again.
            self.struct_instances
                .retain(|name, _| structs.contains(name));
            let made: Vec<String> = self
                .enum_instances
                .keys()
                .filter(|name| !enums.contains(*name))
                .cloned()
                .collect();
            for name in made {
                self.enum_instances.remove(&name);
                self.enum_defs.remove(&name);
            }
        }
        match ty {
            _ if !clean => Probe::Failed,
            Some(ty) => Probe::Typed(ty),
            None => Probe::Untyped,
        }
    }

    /// Hold the type a body produced to its `impl Trait`, then register it.
    fn settle_impl_return(&mut self, site: &Site, held: Held, concrete: Type) {
        if let Some((trait_name, assoc_bindings, span)) = impl_return(site) {
            let (generics, lifetimes) = match site {
                Site::Function(func) => (&func.generics[..], &func.lifetimes[..]),
                Site::Method(_, method) => (&method.generics[..], &method.lifetimes[..]),
            };
            self.enter_generic_scope(generics, lifetimes);
            if self.type_implements_trait(&concrete, &trait_name.name) {
                // `-> impl Trait<Assoc = T>` promises the caller what the concrete type's
                // impl bound the associated type to, so the two must agree.
                let bound = ast_types::TraitBound {
                    trait_name: trait_name.clone(),
                    assoc_bindings: assoc_bindings.to_vec(),
                };
                let resolved = self.resolve_bounds(std::slice::from_ref(&bound));
                for info in &resolved {
                    self.check_assoc_bindings(info, &concrete, span);
                }
            } else {
                self.record_error(TypeError::ImplReturnDoesNotImplement {
                    trait_name: trait_name.name.clone(),
                    ty: concrete.clone(),
                    span,
                });
            }
            self.exit_generic_scope();
        }
        self.restore_signature(site, held, concrete);
    }
}
