//! `impl` blocks: method registration, operator and lang-item traits, and
//! method body checking.
//!
//! One of the declaration-kind modules under `declarations`; each adds methods
//! to the same `impl TypeChecker` block.

use super::{CLONE_TRAIT, COPY_TRAIT, DERIVABLE_TRAITS, DERIVE_ATTRIBUTE, DROP_METHOD, DROP_TRAIT};
use crate::errors::TypeError;
use crate::type_checkers::operator_traits::{is_operator_trait, operator_trait_spec};
use crate::type_checkers::val_else::stmts_diverge;
use crate::type_checkers::{MethodGenerics, TypeChecker};
use crate::types::{FunctionMode, Type};
use ast_types::{ImplDef, Item, SelfParam, StructDef};
use std::collections::HashMap;

impl TypeChecker {
    /// Register all method signatures from an `impl` block into the global
    /// function table under mangled names (`StructName__methodName`).
    ///
    /// `&mut self` is recorded in `mut_self_methods` so call sites can enforce its
    /// exclusive borrow of the receiver, and a consuming `self` in
    /// `consuming_self_methods` so they can record the move of it.
    pub(crate) fn register_impl(&mut self, def: &ImplDef) -> Option<()> {
        let Some(self_ty) = self.impl_target_type(&def.type_name.name) else {
            self.record_error(TypeError::UnknownStruct {
                name: def.type_name.name.clone(),
                span: def.type_name.span,
            });
            return None;
        };

        let struct_name = def.type_name.name.clone();
        self.check_members_against_variants(def, &struct_name);

        // The block's associated-type bindings are in scope for every signature below:
        // a method may write `Self::Item` for what this impl bound it to.
        let saved_assoc = self.enter_impl_assoc(def);

        // Recognize the compiler-known `Drop` lang-item. It is matched by name
        // here exactly like `Copy`/`Clone` derives, without the general trait system.
        if def
            .trait_name
            .as_ref()
            .is_some_and(|t| t.name == DROP_TRAIT)
        {
            self.register_drop_impl(def, &struct_name);
        }

        // Accumulate (method_name, mangled_key) to insert into impl_methods after
        // all mutable borrows of `self` for type resolution are finished.
        let mut method_entries: Vec<(String, String)> = Vec::new();

        let struct_is_copy = self.is_type_copy(&self_ty);

        for method in &def.methods {
            let mangled = format!("{}__{}", struct_name, method.name.name);

            if matches!(method.self_param, Some(SelfParam::RefMut)) {
                self.mut_self_methods.insert(mangled.clone());
            }

            // A `Copy` receiver is duplicated by value, so calling one consumes
            // nothing and the caller keeps its value; only a move-tracked receiver
            // is handed over to the callee.
            if matches!(method.self_param, Some(SelfParam::Owned)) && !struct_is_copy {
                self.consuming_self_methods.insert(mangled.clone());
            }

            if let Some(trait_name) = &def.trait_name
                && !method.generics.is_empty()
            {
                self.record_error(TypeError::GenericMethodInTraitImpl {
                    trait_name: trait_name.name.clone(),
                    method: method.name.name.clone(),
                    span: method.name.span,
                });
            }

            // Build the full parameter type list: implicit `self` first for instance methods.
            self.check_method_generic_names(method);
            let saved_scope = self.enter_method_generics(method);
            let mut param_types: Vec<Type> = Vec::new();
            if method.self_param.is_some() {
                param_types.push(self_ty.clone());
            }
            for param in &method.params {
                if let Some(ty) = self.resolve_type(&param.ty) {
                    param_types.push(ty);
                } else {
                    param_types.push(Type::Unknown);
                }
            }

            let return_type = if let Some(ret_ty) = &method.return_type {
                self.resolve_type(ret_ty).unwrap_or(Type::Void)
            } else {
                Type::Void
            };
            let own_generics = (!method.generics.is_empty()).then(|| self.method_generics(method));
            self.exit_method_generics(saved_scope);

            let func_ty = Type::Function {
                params: param_types,
                ret: Box::new(return_type),
                mode: FunctionMode::Shared,
            };

            if self.functions.contains_key(&mangled) {
                self.record_error(TypeError::FunctionAlreadyDefined {
                    name: mangled.clone(),
                    span: method.name.span,
                });
                continue;
            }

            self.functions.insert(mangled.clone(), func_ty);
            if let Some(own) = own_generics {
                self.generic_methods.insert(mangled.clone(), own);
            }
            method_entries.push((method.name.name.clone(), mangled));
        }

        // Insert collected entries now that all borrows of `self` are released.
        let method_map = self.impl_methods.entry(struct_name.clone()).or_default();
        for (name, mangled) in method_entries {
            method_map.insert(name, mangled);
        }

        // A trait impl (other than the `Drop` lang-item) must conform to the trait's
        // declaration; `Drop` is validated separately above. An operator trait
        // is a compiler-known lang-item like `Drop`, so it is validated and its
        // operator dispatch recorded separately rather than against `self.traits`.
        // The trait's signatures name `Self` for this block's type. For a generic impl
        // that is the template, which is also what `Cell<T>` under its parameters names.
        let saved_self = self.self_type.replace(self_ty.clone());
        if let Some(t) = &def.trait_name {
            if t.name == DROP_TRAIT {
                // handled above
            } else if is_operator_trait(&t.name) {
                self.register_operator_impl(def, &struct_name, &t.name.clone(), struct_is_copy);
            } else {
                self.check_trait_conformance(def, &def.type_name.name.clone(), &t.name.clone());
            }
        }

        self.self_type = saved_self;
        self.self_assoc = saved_assoc;
        Some(())
    }

    /// What a generic method declares itself, read while its parameters are in scope so
    /// the const types and bounds resolve against them.
    fn method_generics(&self, method: &ast_types::MethodDef) -> MethodGenerics {
        let const_types = method
            .generics
            .iter()
            .filter(|g| matches!(g.kind, ast_types::GenericParamKind::Const(_)))
            .map(|g| {
                let ty = self.const_scope.get(&g.name.name).cloned();
                (g.name.name.clone(), ty.unwrap_or(Type::Unknown))
            })
            .collect();
        let bounds = method
            .generics
            .iter()
            .filter(|g| !g.bounds.is_empty())
            .map(|g| {
                let resolved = self.generic_bounds.get(&g.name.name).cloned();
                (g.name.name.clone(), resolved.unwrap_or_default())
            })
            .collect();
        MethodGenerics {
            param_names: method
                .generics
                .iter()
                .map(|g| g.name.name.clone())
                .collect(),
            const_types,
            where_predicates: method.where_predicates.clone(),
            bounds,
        }
    }

    /// The receiver type an `impl` target names: a struct or an enum, generic templates
    /// included, whose base name is registered alongside the concrete ones.
    pub(crate) fn impl_target_type(&self, name: &str) -> Option<Type> {
        if self.struct_defs.contains_key(name) {
            return Some(Type::Struct(name.to_string()));
        }
        if self.enum_defs.contains_key(name) {
            return Some(Type::Enum(name.to_string()));
        }
        // A newtype implements every trait but `Copy` and `Clone` explicitly, operators
        // included, so it takes an `impl` exactly as a struct does.
        if self.newtype_defs.contains_key(name) {
            return Some(Type::Newtype(name.to_string()));
        }
        None
    }

    /// Reject an enum `impl` member that shares a variant's name. `Shape::Circle(r)` is
    /// already a variant construction, so an associated function of that name could never
    /// be called, and a method of that name would read as one at every call site.
    fn check_members_against_variants(&mut self, def: &ImplDef, enum_name: &str) {
        let Some(variants) = self.enum_defs.get(enum_name) else {
            return;
        };
        let clashes: Vec<(String, shared_types::Span)> = def
            .methods
            .iter()
            .filter(|m| variants.iter().any(|v| v.name == m.name.name))
            .map(|m| (m.name.name.clone(), m.name.span))
            .collect();
        for (name, span) in clashes {
            self.record_error(TypeError::ImplMemberNamesVariant {
                enum_name: enum_name.to_string(),
                name,
                span,
            });
        }
    }

    /// Resolve an `impl` block's `type Name = T` bindings and install them as the
    /// associated types in scope, returning the previous scope to restore afterwards.
    ///
    /// The bindings are resolved before they are installed, so a binding cannot name
    /// another one: an associated type stands for a concrete type, not for a chain.
    /// The block's own lifetimes (`impl<'a> S<'a>`) come into scope here too, for the
    /// signatures and bodies of its methods.
    pub(super) fn enter_impl_assoc(&mut self, def: &ImplDef) -> HashMap<String, Type> {
        self.lifetime_scope = def.lifetimes.iter().map(|lt| lt.name.clone()).collect();
        let mut bindings = HashMap::new();
        for (name, ty) in &def.assoc_types {
            if let Some(resolved) = self.resolve_type(ty) {
                bindings.insert(name.name.clone(), resolved);
            }
        }
        std::mem::replace(&mut self.self_assoc, bindings)
    }

    /// Validate an operator-trait impl and record its operator dispatch.
    ///
    /// Operator traits (`Add`, `Sub`, …, `PartialEq`, `Comparable`) are compiler-known
    /// lang-items: the user writes only the `impl`, never a `trait` declaration. Each
    /// impl method whose name matches one the trait provides wires its operator to that
    /// method's return type. The scalar path requires the receiver to be `Copy`; an
    /// `Output` associated type, when present, must equal the method's return type; and a
    /// trait with a supertrait (`Comparable: PartialEq`) requires that impl to exist too.
    pub(super) fn register_operator_impl(
        &mut self,
        def: &ImplDef,
        struct_name: &str,
        trait_name: &str,
        struct_is_copy: bool,
    ) {
        let Some(spec) = operator_trait_spec(trait_name) else {
            return;
        };

        // The scalar operator path is defined for `Copy` receivers only.
        if !struct_is_copy {
            self.record_error(TypeError::OperatorTraitRequiresCopy {
                trait_name: trait_name.to_string(),
                type_name: struct_name.to_string(),
                span: def.type_name.span,
            });
            // The impl is written, only refused: recording it keeps a rule that asks for
            // it, a derive's field rule among them, from reporting it missing as well.
            self.trait_impls
                .insert((trait_name.to_string(), struct_name.to_string()));
            return;
        }

        // The `Output` binding, if the impl declares one, must match the method return.
        let declared_output = def
            .assoc_types
            .iter()
            .find(|(n, _)| n.name == "Output")
            .and_then(|(_, ty)| self.resolve_type(ty));

        for method in &def.methods {
            let bin = spec.binary.iter().find(|(m, _)| *m == method.name.name);
            let un = spec.unary.iter().find(|(m, _)| *m == method.name.name);
            if bin.is_none() && un.is_none() {
                self.record_error(TypeError::NotATraitMethod {
                    trait_name: trait_name.to_string(),
                    method: method.name.name.clone(),
                    span: method.name.span,
                });
                continue;
            }

            let ret = method
                .return_type
                .as_ref()
                .and_then(|t| self.resolve_type(t))
                .unwrap_or(Type::Void);

            let result = if spec.has_output {
                if let Some(out) = &declared_output
                    && !out.is_compatible_with(&ret)
                {
                    self.record_error(TypeError::AssociatedTypeMismatch {
                        trait_name: trait_name.to_string(),
                        expected: out.clone(),
                        found: ret.clone(),
                        span: method.name.span,
                    });
                }
                ret
            } else {
                Type::Bool
            };

            if let Some((_, op)) = bin {
                // A comparison method takes `rhs: &Rhs`; the operand is borrowed at the
                // call, so record the referent as the operand's expected value type.
                let rhs = method
                    .params
                    .first()
                    .and_then(|p| self.resolve_type(&p.ty))
                    .map(|t| t.referent().clone())
                    .unwrap_or(Type::Unknown);
                self.operator_binary_impls.insert(
                    (struct_name.to_string(), *op),
                    crate::type_checkers::OperatorDispatch { rhs, result },
                );
            } else if let Some((_, op)) = un {
                self.operator_unary_impls
                    .insert((struct_name.to_string(), *op), result);
            }
        }

        self.trait_impls
            .insert((trait_name.to_string(), struct_name.to_string()));
    }

    /// Verify each trait impl also has its required supertrait (`Comparable: PartialEq`,
    /// `Copy: Clone`). Runs after every impl is registered so the check is independent of
    /// source order. A derive provides a supertrait as well as an impl does.
    pub(crate) fn check_operator_supertraits(&mut self, items: &[Item]) {
        for item in items {
            let Item::Impl(def) = item else {
                continue;
            };
            let Some(t) = &def.trait_name else { continue };
            let type_name = &def.type_name.name;
            let supertrait = if t.name == COPY_TRAIT {
                // An `impl Copy` already refused for its target has nothing left to need.
                if !self.declares_copy(type_name) {
                    continue;
                }
                CLONE_TRAIT
            } else if let Some(sup) = operator_trait_spec(&t.name).and_then(|spec| spec.supertrait)
            {
                sup
            } else {
                continue;
            };
            let has_super = if supertrait == CLONE_TRAIT {
                // Not `satisfies_bound`: every `Copy` type answers `Clone` there, and the
                // `Copy` impl being checked is what made this type `Copy`.
                self.clone_structs.contains(type_name)
                    || self
                        .trait_impls
                        .contains(&(CLONE_TRAIT.to_string(), type_name.clone()))
            } else {
                self.impl_target_type(type_name)
                    .is_some_and(|ty| self.satisfies_bound(&ty, supertrait))
            };
            if !has_super {
                self.record_error(TypeError::MissingSupertraitImpl {
                    trait_name: t.name.clone(),
                    supertrait: supertrait.to_string(),
                    type_name: type_name.clone(),
                    span: t.span,
                });
            }
        }
    }

    /// Record each hand-written `impl Copy for S` before anything asks whether `S` copies.
    ///
    /// Runs with the struct registration, ahead of every rule that reads `Copy`: the
    /// field validation that follows holds the impl to what `@derive(Copy)` would accept,
    /// and impl registration decides from it whether a `self` method consumes.
    pub(crate) fn register_copy_impls(&mut self, items: &[Item]) {
        for item in items {
            let Item::Impl(def) = item else { continue };
            if def.trait_name.as_ref().is_none_or(|t| t.name != COPY_TRAIT) {
                continue;
            }
            let name = &def.type_name.name;
            if self.generic_structs.contains_key(name) {
                self.register_generic_copy_impl(def);
                continue;
            }
            let refusal = if !self.struct_defs.contains_key(name) {
                Some(
                    "only a struct implements `Copy` by hand; an enum or a newtype is `Copy` exactly when what it holds is",
                )
            } else {
                None
            };
            if let Some(reason) = refusal {
                self.record_error(TypeError::InvalidCopyImpl {
                    type_name: name.clone(),
                    reason: reason.to_string(),
                    span: def.type_name.span,
                });
                continue;
            }
            self.copy_structs.insert(name.clone());
        }
    }

    /// Record `impl<T: Copy> Copy for Cell<T> {}`: which of the struct's parameters the
    /// impl bounds. Lowering decides an instance's `Copy` from the same facts and has no
    /// trait solver, so a bound other than `Copy` is refused rather than half-honoured.
    fn register_generic_copy_impl(&mut self, def: &ImplDef) {
        let name = &def.type_name.name;
        let other_bound = def
            .generics
            .iter()
            .flat_map(|g| &g.bounds)
            .find(|b| b.trait_name.name != COPY_TRAIT);
        if let Some(bound) = other_bound {
            self.record_error(TypeError::InvalidCopyImpl {
                type_name: name.clone(),
                reason: format!(
                    "a generic `impl Copy` may bound its parameters by `Copy` only, not `{}`",
                    bound.trait_name.name
                ),
                span: bound.trait_name.span,
            });
            return;
        }
        let bounds = def
            .type_args
            .iter()
            .map(|arg| {
                let ast_types::Type::Named(id) = arg else {
                    return false;
                };
                def.generics
                    .iter()
                    .any(|g| g.name.name == id.name && !g.bounds.is_empty())
            })
            .collect();
        self.generic_copy_bounds.insert(name.clone(), bounds);
    }

    /// Reject a struct that both derives a trait and declares an `impl` of it.
    ///
    /// The two produce different code, so keeping both would leave one of them silently
    /// unused: the operator dispatch consults the impl first, while `{x:?}` and a map
    /// key read the derive. `@derive(Copy)` derives `Clone` too.
    pub(crate) fn check_derive_impl_conflicts(&mut self, items: &[Item]) {
        let derives: HashMap<&str, Vec<&str>> = items
            .iter()
            .filter_map(|item| match item {
                Item::Struct(def) => Some((def.name.name.as_str(), derived_traits(def))),
                _ => None,
            })
            .collect();
        let mut conflicts: Vec<(String, String, shared_types::Span)> = Vec::new();
        for item in items {
            let Item::Impl(def) = item else {
                continue;
            };
            let Some(t) = &def.trait_name else { continue };
            let struct_name = &def.type_name.name;
            if derives
                .get(struct_name.as_str())
                .is_some_and(|derived| derived.contains(&t.name.as_str()))
            {
                conflicts.push((struct_name.clone(), t.name.clone(), t.span));
            }
        }
        for (struct_name, trait_name, span) in conflicts {
            self.record_error(TypeError::DeriveConflictsWithImpl {
                struct_name,
                trait_name,
                span,
            });
        }
    }

    /// Validate and record an `impl Drop for T` block.
    ///
    /// A Drop type must contain exactly the destructor `drop(&mut self)`: no
    /// parameters, no return, and must not also be `Copy` (a type with a
    /// destructor is moved, never duplicated). The method itself is
    /// registered by the normal `impl` path under `T__drop`; this only enforces the
    /// lang-item shape and records `T` as a Drop type for scope-exit insertion.
    pub(super) fn register_drop_impl(&mut self, def: &ImplDef, struct_name: &str) {
        // The backend runs a user destructor for a struct binding only; an enum's scope exit
        // releases its payload and never looks for a `T__drop`, so the impl would be dead.
        if self.enum_defs.contains_key(struct_name) {
            self.record_error(TypeError::InvalidDropImpl {
                type_name: struct_name.to_string(),
                reason: "an enum may not implement `Drop`; only a struct may".to_string(),
                span: def.type_name.span,
            });
            return;
        }
        self.drop_structs.insert(struct_name.to_string());

        if self.declares_copy(struct_name) {
            self.record_error(TypeError::DropTypeCannotBeCopy {
                type_name: struct_name.to_string(),
                span: def.type_name.span,
            });
        }

        let mut reason: Option<String> = None;
        match def.methods.as_slice() {
            [method] if method.name.name == DROP_METHOD => {
                if !matches!(method.self_param, Some(SelfParam::RefMut)) {
                    reason = Some("`drop` must take `&mut self`".to_string());
                } else if !method.params.is_empty() {
                    reason =
                        Some("`drop` must take no parameters other than `&mut self`".to_string());
                } else if method.return_type.is_some() {
                    reason = Some("`drop` must not return a value".to_string());
                }
            }
            _ => {
                reason = Some(
                    "an `impl Drop` block must contain exactly one method: `drop(&mut self)`"
                        .to_string(),
                );
            }
        }

        if let Some(reason) = reason {
            self.record_error(TypeError::InvalidDropImpl {
                type_name: struct_name.to_string(),
                reason,
                span: def.span,
            });
        }
    }

    /// Type-check the body of each method in an `impl` block.
    pub(crate) fn check_impl(&mut self, def: &ImplDef) {
        let struct_name = def.type_name.name.clone();
        let saved_assoc = self.enter_impl_assoc(def);

        for method in &def.methods {
            let mangled = format!("{}__{}", struct_name, method.name.name);

            // A method whose signature failed to register (a duplicate name, say) never
            // entered `functions`; skip its body rather than check it against nothing.
            let Some(func_ty) = self.functions.get(&mangled).cloned() else {
                continue;
            };

            let (param_types, return_type) = match func_ty {
                Type::Function { params, ret, .. } => (params, *ret),
                _ => continue,
            };

            // The signature in `functions` was resolved with these in scope; the body
            // names the same placeholders.
            let saved_scope = self.enter_method_generics(method);
            self.symbols.push_scope();
            self.current_function_return_type = Some(return_type.clone());
            let warnings_before = self.warnings.len();

            // Bind `self` as a variable of the struct type. A `&mut self` receiver
            // is mutable so the body may assign to `self.field`; `&self` is
            // immutable.
            if method.self_param.is_some() {
                let self_ty = self.impl_target_type(&struct_name).unwrap_or(Type::Unknown);
                let self_mutable = matches!(method.self_param, Some(SelfParam::RefMut));
                let _ = self
                    .symbols
                    .define("self".to_string(), self_ty, self_mutable);
            }
            self.self_is_owned = matches!(method.self_param, Some(SelfParam::Owned));

            // Bind remaining parameters (skip param[0] which is the implicit self).
            let non_self_params = if method.self_param.is_some() && !param_types.is_empty() {
                &param_types[1..]
            } else {
                &param_types[..]
            };

            // `self` (`&self` or `&mut self`) and reference parameters outlive the
            // call, so a returned reference may borrow them (the receiver lifetime
            // is applied to method outputs).
            self.backward_losses = crate::type_checkers::backward::backward_losses(&method.body);
            self.current_fn_outliving = method
                .params
                .iter()
                .zip(non_self_params.iter())
                .filter(|(_, ty)| matches!(ty, Type::Reference { .. }))
                .map(|(param, _)| param.name.name.clone())
                .collect();
            // A consuming receiver is destroyed when the method returns, so a reference
            // into it dangles at the call site; only a borrowed receiver outlives the call.
            if method.self_param.is_some() && !self.self_is_owned {
                self.current_fn_outliving.insert("self".to_string());
            }

            for (param, param_ty) in method.params.iter().zip(non_self_params.iter()) {
                if matches!(param_ty, Type::Unknown) {
                    continue;
                }
                if let Err(dup) =
                    self.symbols
                        .define(param.name.name.clone(), param_ty.clone(), false)
                {
                    self.record_error(TypeError::VariableAlreadyDefined {
                        name: dup,
                        span: param.name.span,
                    });
                }
                self.bind_function_param(&param.name.name, param_ty);
            }

            // The implicit-return tail is checked once, below: see the same rule in
            // `check_function`, where checking it twice moved a by-value argument twice.
            let tail_returns = Self::tail_is_implicit_return(&method.body, &return_type);
            let leading = if tail_returns {
                &method.body[..method.body.len() - 1]
            } else {
                &method.body[..]
            };
            for stmt in leading {
                let _ = self.check_stmt(stmt);
            }

            // Validate the implicit return (same rule as free functions), or report that
            // the method has none: without this the backend emits a body that runs off
            // its own end at runtime.
            if tail_returns {
                self.check_implicit_return(&method.body, &return_type);
            } else if !matches!(return_type, Type::Void) && !stmts_diverge(&method.body) {
                self.record_error(TypeError::MissingReturn {
                    expected: return_type.clone(),
                    span: method.name.span,
                });
            }

            self.drop_allowed_warnings(warnings_before, &method.attributes);
            self.symbols.pop_scope();
            self.current_function_return_type = None;
            self.current_fn_outliving.clear();
            self.self_is_owned = false;
            self.exit_method_generics(saved_scope);
        }

        self.self_assoc = saved_assoc;
    }
}

/// The traits a struct's `@derive` attributes name, `Clone` included when `Copy` is.
/// Names outside the derivable set are left out: they are reported where the derive is.
fn derived_traits(def: &StructDef) -> Vec<&str> {
    let mut derived: Vec<&str> = def
        .attributes
        .iter()
        .filter(|attr| attr.name.name == DERIVE_ATTRIBUTE)
        .flat_map(|attr| &attr.args)
        .filter_map(|arg| DERIVABLE_TRAITS.iter().find(|d| **d == arg.name).copied())
        .collect();
    if derived.contains(&COPY_TRAIT) {
        derived.push(CLONE_TRAIT);
    }
    derived
}
