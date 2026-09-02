//! Trait declarations and the conformance check an `impl` must satisfy.
//!
//! One of the declaration-kind modules under `declarations`; each adds methods
//! to the same `impl TypeChecker` block.

use super::is_builtin_type_name;
use crate::errors::TypeError;
use crate::type_checkers::resolution::SELF_ASSOC_PREFIX;
use crate::type_checkers::{TraitInfo, TraitMethodSig, TypeChecker};
use crate::types::Type;
use ast_types::{ImplDef, TraitDef};
use shared_types::Identifier;
use std::collections::HashMap;

impl TypeChecker {
    /// Register a trait declaration's associated types and method signatures.
    ///
    /// Each signature is resolved in the trait's (non-generic) scope. A position naming
    /// an associated type is left as [`Type::Unknown`] here rather than resolved: the
    /// declaration says which member exists, and only an implementor's binding says what
    /// it is. The signature as written is kept so conformance can resolve it per impl.
    /// A duplicate trait name or method is rejected.
    pub(crate) fn register_trait(&mut self, def: &TraitDef) {
        if self.traits.contains_key(&def.name.name) || is_builtin_type_name(&def.name.name) {
            self.record_error(TypeError::TraitAlreadyDefined {
                trait_name: def.name.name.clone(),
                span: def.name.span,
            });
            return;
        }

        let declared: Vec<String> = def.assoc_types.iter().map(|a| a.name.clone()).collect();
        let mut methods: HashMap<String, TraitMethodSig> = HashMap::new();
        for m in &def.methods {
            let mut named = Vec::new();
            for ty in m.params.iter().map(|p| &p.ty).chain(m.return_type.iter()) {
                collect_self_assoc(ty, &mut named);
            }
            for assoc in &named {
                if !declared.contains(&assoc.name) {
                    self.record_error(TypeError::UnknownAssociatedType {
                        trait_name: def.name.name.clone(),
                        name: assoc.name.clone(),
                        span: assoc.span,
                    });
                }
            }
            let params: Vec<Type> = m
                .params
                .iter()
                .map(|p| self.resolve_trait_sig_type(&p.ty))
                .collect();
            let ret = m
                .return_type
                .as_ref()
                .map(|t| self.resolve_trait_sig_type(t))
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
                    decl: m.clone(),
                    uses_assoc: !named.is_empty(),
                },
            );
        }

        self.traits.insert(
            def.name.name.clone(),
            TraitInfo {
                methods,
                assoc_types: declared,
            },
        );
    }

    /// Resolve one position of a trait's declared signature. An associated-type position
    /// has no binding at the declaration, so it carries no information rather than a
    /// wrong one; every other position resolves and reports as usual.
    fn resolve_trait_sig_type(&mut self, ty: &ast_types::Type) -> Type {
        let mut named = Vec::new();
        collect_self_assoc(ty, &mut named);
        if !named.is_empty() {
            return Type::Unknown;
        }
        self.resolve_type(ty).unwrap_or(Type::Unknown)
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

        // The associated types: every declared one bound exactly once, and nothing bound
        // that was never declared. The bindings themselves are already in scope
        // (`self.self_assoc`), which is what lets the signature comparison below resolve
        // a trait's `Self::Item` to what this impl chose.
        for (name, ty) in &def.assoc_types {
            if !info.assoc_types.contains(&name.name) {
                errors.push(TypeError::UnknownAssociatedType {
                    trait_name: trait_name.to_string(),
                    name: name.name.clone(),
                    span: ty.span(),
                });
            }
        }
        for declared in &info.assoc_types {
            if !def.assoc_types.iter().any(|(n, _)| &n.name == declared) {
                errors.push(TypeError::MissingAssociatedType {
                    trait_name: trait_name.to_string(),
                    type_name: struct_name.to_string(),
                    name: declared.clone(),
                    span: def.type_name.span,
                });
            }
        }
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
        // The bindings are in scope as `self_assoc` for exactly this block; recording
        // them keyed by the pair is what lets a `Trait<Assoc = T>` bound elsewhere ask
        // what this impl chose.
        let bindings: HashMap<String, Type> = info
            .assoc_types
            .iter()
            .filter_map(|name| {
                self.self_assoc
                    .get(name)
                    .map(|ty| (name.clone(), ty.clone()))
            })
            .collect();
        self.impl_assoc
            .insert((trait_name.to_string(), struct_name.to_string()), bindings);
        self.trait_impls
            .insert((trait_name.to_string(), struct_name.to_string()));
    }

    /// Compare one impl method's signature against its trait declaration, returning a
    /// human-readable reason when they differ. Both sides are resolved in the impl's
    /// scope — the trait's signature as written, not as registered — so an associated-type
    /// position is compared as the type this impl bound it to, and an impl may spell that
    /// position either way.
    pub(super) fn trait_signature_mismatch(
        &mut self,
        method: &ast_types::MethodDef,
        sig: &TraitMethodSig,
    ) -> Option<String> {
        if method.self_param != sig.self_param {
            return Some("receiver (`self`) form differs from the trait".to_string());
        }
        if method.params.len() != sig.decl.params.len() {
            return Some(format!(
                "expected {} parameter(s), found {}",
                sig.decl.params.len(),
                method.params.len()
            ));
        }
        for (p, declared) in method.params.iter().zip(sig.decl.params.iter()) {
            let expected = self.resolve_type(&declared.ty).unwrap_or(Type::Unknown);
            let got = self.resolve_type(&p.ty).unwrap_or(Type::Unknown);
            if !got.is_compatible_with(&expected) {
                return Some(format!(
                    "parameter '{}' has type {}, trait requires {}",
                    p.name.name, got, expected
                ));
            }
        }
        let expected_ret = sig
            .decl
            .return_type
            .as_ref()
            .map(|t| self.resolve_type(t).unwrap_or(Type::Unknown))
            .unwrap_or(Type::Void);
        let ret = method
            .return_type
            .as_ref()
            .map(|t| self.resolve_type(t).unwrap_or(Type::Void))
            .unwrap_or(Type::Void);
        if !ret.is_compatible_with(&expected_ret) {
            return Some(format!(
                "return type is {}, trait requires {}",
                ret, expected_ret
            ));
        }
        None
    }
}

/// Collect every associated-type path an annotation names, nested positions included:
/// `Option<Self::Item>` names `Item` just as a bare `Self::Item` does.
pub(crate) fn collect_self_assoc(ty: &ast_types::Type, out: &mut Vec<Identifier>) {
    match ty {
        ast_types::Type::Named(ident) => {
            if let Some(assoc) = ident.name.strip_prefix(SELF_ASSOC_PREFIX) {
                out.push(Identifier {
                    name: assoc.to_string(),
                    span: ident.span,
                });
            }
        }
        ast_types::Type::Reference { inner, .. } => collect_self_assoc(inner, out),
        ast_types::Type::Array { element, .. } | ast_types::Type::Slice { element, .. } => {
            collect_self_assoc(element, out)
        }
        ast_types::Type::Tuple { elements, .. } => {
            for element in elements {
                collect_self_assoc(element, out);
            }
        }
        ast_types::Type::Generic { args, .. } => {
            for arg in args {
                if let ast_types::GenericArg::Type(inner) = arg {
                    collect_self_assoc(inner, out);
                }
            }
        }
        ast_types::Type::Function { params, ret, .. } => {
            for param in params {
                collect_self_assoc(param, out);
            }
            collect_self_assoc(ret, out);
        }
        ast_types::Type::ImplTrait { .. }
        | ast_types::Type::DynTrait { .. }
        | ast_types::Type::Tensor { .. } => {}
    }
}
