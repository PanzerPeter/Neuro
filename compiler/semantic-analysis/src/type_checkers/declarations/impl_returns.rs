//! Return-position `impl Trait`: the one concrete type a function's body produces,
//! read by checking the body once every other signature is registered.
//!
//! One of the declaration-kind modules under `declarations`; each adds methods
//! to the same `impl TypeChecker` block.

use crate::errors::TypeError;
use crate::type_checkers::{GenericFnSig, TypeChecker};
use crate::types::Type;
use ast_types::{FunctionDef, Item, Stmt};
use shared_types::{Identifier, Span};
use std::collections::HashSet;

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

/// The `impl Trait` a function returns: its trait, its `Assoc = T` bindings and span.
fn impl_return(func: &FunctionDef) -> Option<(&Identifier, &AssocBindings, Span)> {
    match &func.return_type {
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
        let mut pending = Vec::new();
        for item in items {
            let Item::Function(func) = item else { continue };
            let Some((trait_name, _, span)) = impl_return(func) else {
                continue;
            };
            if !self.traits.contains_key(&trait_name.name) {
                self.record_error(TypeError::UnknownTrait {
                    trait_name: trait_name.name.clone(),
                    span,
                });
                continue;
            }
            if let Some(held) = self.take_signature(func) {
                pending.push((func, held));
            }
        }
        // ponytail: each round re-checks every unresolved body, quadratic in a chain of
        // `impl Trait` functions calling each other; ordering them by call graph is one pass.
        loop {
            let before = pending.len();
            let mut unresolved = Vec::with_capacity(before);
            for (func, held) in pending {
                match self.probe_body_type(func, held.params()) {
                    Probe::Typed(ty) => self.settle_impl_return(func, held, ty),
                    Probe::Untyped | Probe::Failed => unresolved.push((func, held)),
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
        for (func, held) in pending {
            leftovers.push((func, held.params().to_vec()));
            self.restore_signature(func, held, Type::Unknown);
        }
        for (func, params) in leftovers {
            if let Probe::Failed = self.probe_body_type(func, &params) {
                continue;
            }
            if let Some((trait_name, _, span)) = impl_return(func) {
                self.record_error(TypeError::ImplReturnNotInferable {
                    trait_name: trait_name.name.clone(),
                    span,
                });
            }
        }
    }

    fn take_signature(&mut self, func: &FunctionDef) -> Option<Held> {
        if func.generics.is_empty() {
            self.functions.remove(&func.name.name).map(Held::Plain)
        } else {
            self.generic_funcs
                .remove(&func.name.name)
                .map(|sig| Held::Generic(Box::new(sig)))
        }
    }

    fn restore_signature(&mut self, func: &FunctionDef, held: Held, ret: Type) {
        match held {
            Held::Plain(Type::Function { params, mode, .. }) => {
                self.functions.insert(
                    func.name.name.clone(),
                    Type::Function {
                        params,
                        ret: Box::new(ret),
                        mode,
                    },
                );
            }
            Held::Plain(other) => {
                self.functions.insert(func.name.name.clone(), other);
            }
            Held::Generic(mut sig) => {
                sig.ret = ret;
                self.generic_funcs.insert(func.name.name.clone(), *sig);
            }
        }
    }

    /// Check `func`'s body for the type it ends with. Nothing the check reports is kept:
    /// pass 4 checks the body again, against the type found here.
    fn probe_body_type(&mut self, func: &FunctionDef, param_types: &[Type]) -> Probe {
        let errors = self.errors.len();
        let warnings = self.warnings.len();
        let structs: HashSet<String> = self.struct_instances.keys().cloned().collect();
        let enums: HashSet<String> = self.enum_instances.keys().cloned().collect();
        let module = std::mem::replace(&mut self.current_module, func.module);
        self.enter_generic_scope(&func.generics, &func.lifetimes);
        self.enter_function_body(func, param_types, &Type::Unknown);
        let ty = func.body.split_last().and_then(|(last, init)| {
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
        self.leave_function_body();
        self.exit_generic_scope();
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

    /// Hold the type `func`'s body produced to its `impl Trait`, then register it.
    fn settle_impl_return(&mut self, func: &FunctionDef, held: Held, concrete: Type) {
        if let Some((trait_name, assoc_bindings, span)) = impl_return(func) {
            self.enter_generic_scope(&func.generics, &func.lifetimes);
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
        self.restore_signature(func, held, concrete);
    }
}
