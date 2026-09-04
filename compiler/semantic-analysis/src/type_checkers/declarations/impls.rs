//! `impl` blocks: method registration, operator and lang-item traits, and
//! method body checking.
//!
//! One of the declaration-kind modules under `declarations`; each adds methods
//! to the same `impl TypeChecker` block.

use super::{DEBUG_TRAIT, DROP_METHOD, DROP_TRAIT, PARTIAL_EQ_TRAIT};
use crate::errors::TypeError;
use crate::type_checkers::collections::{HASHABLE_TRAIT, HASH_METHOD};
use crate::type_checkers::operator_traits::{is_operator_trait, operator_trait_spec};
use crate::type_checkers::val_else::stmts_diverge;
use crate::type_checkers::TypeChecker;
use crate::types::Type;
use ast_types::{ImplDef, SelfParam};
use std::collections::HashMap;

impl TypeChecker {
    /// Register all method signatures from an `impl` block into the global
    /// function table under mangled names (`StructName__methodName`).
    ///
    /// Consuming `self` is rejected here so it never reaches codegen; `&mut self`
    /// is recorded in `mut_self_methods` so call sites can enforce its exclusive
    /// borrow of the receiver.
    pub(crate) fn register_impl(&mut self, def: &ImplDef) -> Option<()> {
        if !self.struct_defs.contains_key(&def.type_name.name) {
            self.record_error(TypeError::UnknownStruct {
                name: def.type_name.name.clone(),
                span: def.type_name.span,
            });
            return None;
        }

        let struct_name = def.type_name.name.clone();

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

        let struct_is_copy = self.copy_structs.contains(&struct_name);

        for method in &def.methods {
            // Consuming `self` still needs the by-value struct ABI for non-`Copy` types,
            // so reject it there. A `Copy` struct is duplicated by value, which is
            // ABI-identical to `&self`, so an owned `self` is accepted — this is what lets
            // an operator-trait method `func add(self, ...)` run on the scalar path
            // `&mut self` is supported and recorded below.
            if matches!(method.self_param, Some(SelfParam::Owned)) && !struct_is_copy {
                self.errors.push(TypeError::UnsupportedSelfParam {
                    type_name: struct_name.clone(),
                    self_param: "self".to_string(),
                    span: method.span,
                });
                continue;
            }

            let mangled = format!("{}__{}", struct_name, method.name.name);

            if matches!(method.self_param, Some(SelfParam::RefMut)) {
                self.mut_self_methods.insert(mangled.clone());
            }

            // Build the full parameter type list: implicit `self` first for instance methods.
            let mut param_types: Vec<Type> = Vec::new();
            if method.self_param.is_some() {
                param_types.push(Type::Struct(struct_name.clone()));
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

            let func_ty = Type::Function {
                params: param_types,
                ret: Box::new(return_type),
            };

            if self.functions.contains_key(&mangled) {
                self.record_error(TypeError::FunctionAlreadyDefined {
                    name: mangled.clone(),
                    span: method.name.span,
                });
                continue;
            }

            self.functions.insert(mangled.clone(), func_ty);
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
        if let Some(t) = &def.trait_name {
            if t.name == DROP_TRAIT {
                // handled above
            } else if t.name == HASHABLE_TRAIT {
                self.register_hashable_impl(def, &struct_name);
            } else if is_operator_trait(&t.name) {
                self.register_operator_impl(def, &struct_name, &t.name.clone(), struct_is_copy);
            } else {
                self.check_trait_conformance(def, &def.type_name.name.clone(), &t.name.clone());
            }
        }

        self.self_assoc = saved_assoc;
        Some(())
    }

    /// Resolve an `impl` block's `type Name = T` bindings and install them as the
    /// associated types in scope, returning the previous scope to restore afterwards.
    ///
    /// The bindings are resolved before they are installed, so a binding cannot name
    /// another one — an associated type stands for a concrete type, not for a chain.
    pub(super) fn enter_impl_assoc(&mut self, def: &ImplDef) -> HashMap<String, Type> {
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
    /// lang-items — the user writes only the `impl`, never a `trait` declaration. Each
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
                if let Some(out) = &declared_output {
                    if !out.is_compatible_with(&ret) {
                        self.record_error(TypeError::AssociatedTypeMismatch {
                            trait_name: trait_name.to_string(),
                            expected: out.clone(),
                            found: ret.clone(),
                            span: method.name.span,
                        });
                    }
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

    /// Verify each operator-trait impl also provides any required supertrait impl
    /// (`Comparable: PartialEq`). Runs after every impl is registered so the check
    /// is independent of source order.
    pub(crate) fn check_operator_supertraits(&mut self, items: &[ast_types::Item]) {
        for item in items {
            let ast_types::Item::Impl(def) = item else {
                continue;
            };
            let Some(t) = &def.trait_name else { continue };
            let Some(spec) = operator_trait_spec(&t.name) else {
                continue;
            };
            if let Some(sup) = spec.supertrait {
                let has_super = self
                    .trait_impls
                    .contains(&(sup.to_string(), def.type_name.name.clone()));
                if !has_super {
                    self.record_error(TypeError::MissingSupertraitImpl {
                        trait_name: t.name.clone(),
                        supertrait: sup.to_string(),
                        type_name: def.type_name.name.clone(),
                        span: t.span,
                    });
                }
            }
        }
    }

    /// Reject a struct that both derives a trait and declares an `impl` of it.
    ///
    /// The two produce different code — the derive compares fields inline, the impl
    /// routes the operator through a method — and the operator dispatch consults the
    /// impl first, so the derive would be silently outranked.
    pub(crate) fn check_derive_impl_conflicts(&mut self, items: &[ast_types::Item]) {
        let mut conflicts: Vec<(String, &'static str, shared_types::Span)> = Vec::new();
        for item in items {
            let ast_types::Item::Impl(def) = item else {
                continue;
            };
            let Some(t) = &def.trait_name else { continue };
            let struct_name = &def.type_name.name;
            let derived = match t.name.as_str() {
                PARTIAL_EQ_TRAIT if self.struct_is_partial_eq(struct_name) => PARTIAL_EQ_TRAIT,
                DEBUG_TRAIT if self.struct_is_debug(struct_name) => DEBUG_TRAIT,
                _ => continue,
            };
            conflicts.push((struct_name.clone(), derived, t.span));
        }
        for (struct_name, trait_name, span) in conflicts {
            self.record_error(TypeError::DeriveConflictsWithImpl {
                struct_name,
                trait_name: trait_name.to_string(),
                span,
            });
        }
    }

    /// Validate and record an `impl Drop for T` block.
    ///
    /// A Drop type must contain exactly the destructor `drop(&mut self)` — no
    /// parameters, no return — and must not also be `Copy` (a type with a
    /// destructor is moved, never duplicated). The method itself is
    /// registered by the normal `impl` path under `T__drop`; this only enforces the
    /// lang-item shape and records `T` as a Drop type for scope-exit insertion.
    pub(super) fn register_drop_impl(&mut self, def: &ImplDef, struct_name: &str) {
        if self.copy_structs.contains(struct_name) {
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

    /// Validate an `impl Hashable for T` block and record the impl.
    ///
    /// `Hashable` is a compiler-known lang-item like `Drop` and the operator traits: it
    /// exists so a user struct can be a `HashMap` key, and the generated probe sequence
    /// calls `T__hash` directly. The shape is therefore fixed — one `hash(&self) -> u64`
    /// method, taking the receiver by reference so hashing never moves the key.
    pub(super) fn register_hashable_impl(&mut self, def: &ImplDef, struct_name: &str) {
        let valid = match def.methods.as_slice() {
            [method] if method.name.name == HASH_METHOD => {
                matches!(method.self_param, Some(SelfParam::Ref))
                    && method.params.is_empty()
                    && method
                        .return_type
                        .as_ref()
                        .and_then(|t| self.resolve_type(t))
                        .is_some_and(|t| t == Type::U64)
            }
            _ => false,
        };

        if !valid {
            self.record_error(TypeError::InvalidHashableImpl {
                type_name: struct_name.to_string(),
                span: def.span,
            });
            return;
        }

        self.trait_impls
            .insert((HASHABLE_TRAIT.to_string(), struct_name.to_string()));
    }

    /// Type-check the body of each method in an `impl` block.
    pub(crate) fn check_impl(&mut self, def: &ImplDef) {
        let struct_name = def.type_name.name.clone();
        let saved_assoc = self.enter_impl_assoc(def);

        for method in &def.methods {
            let mangled = format!("{}__{}", struct_name, method.name.name);

            // An owned `self` on a non-`Copy` struct was rejected during registration and
            // never entered `functions`; skip it here. A `Copy` receiver's owned `self` is
            // registered and checked exactly like `&self`.
            let func_ty = match self.functions.get(&mangled).cloned() {
                Some(ty) => ty,
                None => continue,
            };

            let (param_types, return_type) = match func_ty {
                Type::Function { params, ret } => (params, *ret),
                _ => continue,
            };

            self.symbols.push_scope();
            self.current_function_return_type = Some(return_type.clone());

            // Bind `self` as a variable of the struct type. A `&mut self` receiver
            // is mutable so the body may assign to `self.field`; `&self` is
            // immutable.
            if method.self_param.is_some() {
                let self_ty = Type::Struct(struct_name.clone());
                let self_mutable = matches!(method.self_param, Some(SelfParam::RefMut));
                let _ = self
                    .symbols
                    .define("self".to_string(), self_ty, self_mutable);
            }

            // Bind remaining parameters (skip param[0] which is the implicit self).
            let non_self_params = if method.self_param.is_some() && !param_types.is_empty() {
                &param_types[1..]
            } else {
                &param_types[..]
            };

            // `self` (`&self` or `&mut self`) and reference parameters outlive the
            // call, so a returned reference may borrow them (the receiver lifetime
            // is applied to method outputs).
            self.current_fn_outliving = method
                .params
                .iter()
                .zip(non_self_params.iter())
                .filter(|(_, ty)| matches!(ty, Type::Reference { .. }))
                .map(|(param, _)| param.name.name.clone())
                .collect();
            if method.self_param.is_some() {
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
            }

            // The implicit-return tail is checked once, below — see the same rule in
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

            self.symbols.pop_scope();
            self.current_function_return_type = None;
            self.current_fn_outliving.clear();
        }

        self.self_assoc = saved_assoc;
    }
}
