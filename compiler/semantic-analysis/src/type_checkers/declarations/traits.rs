//! Trait declarations and the conformance check an `impl` must satisfy.
//!
//! One of the declaration-kind modules under `declarations`; each adds methods
//! to the same `impl TypeChecker` block.

use super::is_builtin_type_name;
use crate::errors::TypeError;
use crate::type_checkers::{TraitInfo, TraitMethodSig, TypeChecker};
use crate::types::Type;
use ast_types::{ImplDef, TraitDef};
use std::collections::HashMap;

impl TypeChecker {
    /// Register a trait declaration's method signatures.
    ///
    /// Each signature is resolved in the trait's (non-generic) scope; `Self`-typed and
    /// associated-type positions are not supported this phase (they land with operator
    /// traits and dispatch). A duplicate trait name or method is rejected.
    pub(crate) fn register_trait(&mut self, def: &TraitDef) {
        if self.traits.contains_key(&def.name.name) || is_builtin_type_name(&def.name.name) {
            self.record_error(TypeError::TraitAlreadyDefined {
                trait_name: def.name.name.clone(),
                span: def.name.span,
            });
            return;
        }

        let mut methods: HashMap<String, TraitMethodSig> = HashMap::new();
        for m in &def.methods {
            let params: Vec<Type> = m
                .params
                .iter()
                .map(|p| self.resolve_type(&p.ty).unwrap_or(Type::Unknown))
                .collect();
            let ret = m
                .return_type
                .as_ref()
                .map(|t| self.resolve_type(t).unwrap_or(Type::Void))
                .unwrap_or(Type::Void);
            if methods.contains_key(&m.name.name) {
                self.record_error(TypeError::FunctionAlreadyDefined {
                    name: format!("{}::{}", def.name.name, m.name.name),
                    span: m.name.span,
                });
                continue;
            }
            methods.insert(
                m.name.name.clone(),
                TraitMethodSig {
                    self_param: m.self_param.clone(),
                    params,
                    ret,
                    required: m.default_body.is_none(),
                },
            );
        }

        self.traits
            .insert(def.name.name.clone(), TraitInfo { methods });
    }

    /// Validate an `impl Trait for Type` block against the trait's declaration and
    /// record the `(trait, type)` pair so generic bounds on `Type` are satisfied.
    ///
    /// Runs after the impl's methods are registered as ordinary inherent methods (the
    /// parser has already injected any omitted default methods), so conformance is a
    /// signature comparison: every required method present, every impl method a trait
    /// member, and each shared method's signature matching the trait's.
    pub(super) fn check_trait_conformance(
        &mut self,
        def: &ImplDef,
        struct_name: &str,
        trait_name: &str,
    ) {
        let info = match self.traits.get(trait_name).cloned() {
            Some(i) => i,
            None => {
                self.record_error(TypeError::UnknownTrait {
                    trait_name: trait_name.to_string(),
                    span: def.trait_name.as_ref().map(|t| t.span).unwrap_or(def.span),
                });
                return;
            }
        };

        // Compare each impl method against its trait declaration. Collect diagnostics
        // first so the immutable borrow of `self.traits` is released before recording.
        let mut errors: Vec<TypeError> = Vec::new();
        for method in &def.methods {
            let sig = match info.methods.get(&method.name.name) {
                Some(s) => s,
                None => {
                    errors.push(TypeError::NotATraitMethod {
                        trait_name: trait_name.to_string(),
                        method: method.name.name.clone(),
                        span: method.name.span,
                    });
                    continue;
                }
            };
            if let Some(detail) = self.trait_signature_mismatch(method, sig) {
                errors.push(TypeError::TraitMethodSignatureMismatch {
                    trait_name: trait_name.to_string(),
                    type_name: struct_name.to_string(),
                    method: method.name.name.clone(),
                    detail,
                    span: method.name.span,
                });
            }
        }

        // Every required trait method must be provided (defaults were injected already).
        for (mname, sig) in &info.methods {
            if sig.required && !def.methods.iter().any(|m| &m.name.name == mname) {
                errors.push(TypeError::MissingTraitMethod {
                    trait_name: trait_name.to_string(),
                    type_name: struct_name.to_string(),
                    method: mname.clone(),
                    span: def.type_name.span,
                });
            }
        }

        for e in errors {
            self.record_error(e);
        }
        self.trait_impls
            .insert((trait_name.to_string(), struct_name.to_string()));
    }

    /// Compare one impl method's signature against its trait declaration, returning a
    /// human-readable reason when they differ. Parameter and return types are
    /// resolved in the (non-generic) impl scope, matching the trait's resolution.
    pub(super) fn trait_signature_mismatch(
        &mut self,
        method: &ast_types::MethodDef,
        sig: &TraitMethodSig,
    ) -> Option<String> {
        if method.self_param != sig.self_param {
            return Some("receiver (`self`) form differs from the trait".to_string());
        }
        if method.params.len() != sig.params.len() {
            return Some(format!(
                "expected {} parameter(s), found {}",
                sig.params.len(),
                method.params.len()
            ));
        }
        for (p, expected) in method.params.iter().zip(sig.params.iter()) {
            let got = self.resolve_type(&p.ty).unwrap_or(Type::Unknown);
            if !got.is_compatible_with(expected) {
                return Some(format!(
                    "parameter '{}' has type {}, trait requires {}",
                    p.name.name, got, expected
                ));
            }
        }
        let ret = method
            .return_type
            .as_ref()
            .map(|t| self.resolve_type(t).unwrap_or(Type::Void))
            .unwrap_or(Type::Void);
        if !ret.is_compatible_with(&sig.ret) {
            return Some(format!(
                "return type is {}, trait requires {}",
                ret, sig.ret
            ));
        }
        None
    }
}
