//! Trait declarations and the conformance check an `impl` must satisfy.
//!
//! One of the declaration-kind modules under `declarations`; each adds methods
//! to the same `impl TypeChecker` block.

use super::{DROP_TRAIT, is_builtin_type_name};
use crate::errors::TypeError;
use crate::type_checkers::expressions::const_predicates::same_predicate;
use crate::type_checkers::operator_traits::{is_operator_trait, operator_trait_spec};
use crate::type_checkers::resolution::{SELF_ASSOC_PREFIX, SELF_TYPE_NAME};
use crate::type_checkers::{BoundInfo, GenericFnSig, TraitInfo, TraitMethodSig, TypeChecker};
use crate::types::Type;
use ast_types::{ImplDef, Item, TraitDef, TraitMethod};
use shared_types::{Identifier, Span};
use std::collections::HashMap;

impl TypeChecker {
    /// Register a trait declaration's associated types and method signatures.
    ///
    /// Each signature is resolved in the trait's (non-generic) scope. A position naming
    /// `Self` or an associated type is left as [`Type::Unknown`] here rather than resolved:
    /// the declaration says which member exists, and only an implementor says what it is. The signature as written is kept so conformance can resolve it per impl.
    /// A duplicate trait name or method is rejected.
    pub(crate) fn register_trait(&mut self, def: &TraitDef) {
        let name = &def.name.name;
        if self.traits.contains_key(name)
            || is_builtin_type_name(name)
            || self.struct_defs.contains_key(name)
            || self.generic_structs.contains_key(name)
            || self.enum_defs.contains_key(name)
            || self.newtype_defs.contains_key(name)
        {
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
            let mut names_self_type = false;
            for ty in signature_types(m) {
                collect_self_assoc(ty, &mut named);
                names_self_type |= names_self(ty);
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
            let (params, ret) = if declares_generics(m) {
                // An undeclared `Self::X` is reported above, so it is held open like the
                // declared ones rather than reported a second time as unbound.
                let mut open: Vec<String> = declared.clone();
                open.extend(named.iter().map(|a| a.name.clone()));
                let sig = self.open_trait_signature(m, &open);
                (sig.params, sig.ret)
            } else {
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
                (params, ret)
            };
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
                    resolved_per_use: names_self_type || !named.is_empty(),
                },
            );
        }

        self.traits.insert(
            def.name.name.clone(),
            TraitInfo {
                methods,
                assoc_types: declared,
                supertraits: Vec::new(),
            },
        );
    }

    /// Resolve a trait's `: A + B` list. A supertrait is a declared trait or one the
    /// compiler knows by name; a typo here would otherwise read as a trait nothing can
    /// implement.
    pub(crate) fn register_supertraits(&mut self, def: &TraitDef) {
        let mut written = Vec::new();
        for bound in &def.supertraits {
            let name = &bound.trait_name;
            if self.traits.contains_key(&name.name)
                || is_operator_trait(&name.name)
                || name.name == DROP_TRAIT
            {
                written.push(bound.clone());
            } else {
                self.record_error(TypeError::UnknownTrait {
                    trait_name: name.name.clone(),
                    span: name.span,
                });
            }
        }
        let assoc_types = self
            .traits
            .get(&def.name.name)
            .map(|info| info.assoc_types.clone())
            .unwrap_or_default();
        let supertraits = self.with_open_self(&assoc_types, |this| this.resolve_bounds(&written));
        if let Some(info) = self.traits.get_mut(&def.name.name) {
            info.supertraits = supertraits;
        }
    }

    /// Report each trait that is its own supertrait, directly or through others, and cut
    /// its list: every walk of the relation after this one can then assume it terminates.
    pub(crate) fn reject_supertrait_cycles(&mut self, items: &[Item]) {
        let cyclic: Vec<&TraitDef> = items
            .iter()
            .filter_map(|item| match item {
                Item::Trait(def) => Some(def),
                _ => None,
            })
            .filter(|def| {
                let name = &def.name.name;
                self.supertraits_of(name)
                    .iter()
                    .any(|sup| self.trait_closure(&sup.trait_name).contains(name))
            })
            .collect();
        for def in cyclic {
            self.record_error(TypeError::SupertraitCycle {
                trait_name: def.name.name.clone(),
                span: def.name.span,
            });
            if let Some(info) = self.traits.get_mut(&def.name.name) {
                info.supertraits.clear();
            }
        }
    }

    /// The direct supertraits of a trait, `Comparable: PartialEq` among them, which has no
    /// declaration to carry it.
    pub(crate) fn supertraits_of(&self, trait_name: &str) -> Vec<BoundInfo> {
        if let Some(info) = self.traits.get(trait_name) {
            return info.supertraits.clone();
        }
        operator_trait_spec(trait_name)
            .and_then(|spec| spec.supertrait)
            .map(|sup| BoundInfo {
                trait_name: sup.to_string(),
                assoc: Vec::new(),
            })
            .into_iter()
            .collect()
    }

    /// `trait_name` and every trait it reaches through supertraits, nearest first.
    pub(crate) fn trait_closure(&self, trait_name: &str) -> Vec<String> {
        let mut closure = vec![trait_name.to_string()];
        let mut next = 0;
        while next < closure.len() {
            for sup in self.supertraits_of(&closure[next]) {
                if !closure.contains(&sup.trait_name) {
                    closure.push(sup.trait_name);
                }
            }
            next += 1;
        }
        closure
    }

    /// Every trait a type parameter's bounds reach, supertraits included.
    pub(crate) fn param_traits(&self, param: &str) -> Vec<String> {
        self.generic_bounds
            .get(param)
            .into_iter()
            .flatten()
            .flat_map(|bound| self.trait_closure(&bound.trait_name))
            .collect()
    }

    /// A type parameter's bounds with each supertrait spelled out as a bound of its own,
    /// its bindings filled in from `param` and from the subtrait's bound.
    pub(crate) fn param_bounds(&mut self, param: &str, span: Span) -> Vec<BoundInfo> {
        let mut bounds = self.generic_bounds.get(param).cloned().unwrap_or_default();
        let mut next = 0;
        while next < bounds.len() {
            let sub = bounds[next].clone();
            next += 1;
            let mut subst: HashMap<String, Type> = sub
                .assoc
                .iter()
                .map(|(name, ty)| (format!("{SELF_ASSOC_PREFIX}{name}"), ty.clone()))
                .collect();
            subst.insert(SELF_TYPE_NAME.to_string(), Type::Generic(param.to_string()));
            // The bound may leave one of the subtrait's own associated types open, and the
            // type behind a binding through it is then not known here.
            let declared = self
                .traits
                .get(&sub.trait_name)
                .map(|info| info.assoc_types.clone())
                .unwrap_or_default();
            for name in declared {
                subst
                    .entry(format!("{SELF_ASSOC_PREFIX}{name}"))
                    .or_insert(Type::Unknown);
            }
            for mut sup in self.supertraits_of(&sub.trait_name) {
                for (_, ty) in &mut sup.assoc {
                    *ty = self.substitute_generic(ty, &subst, span);
                }
                match bounds.iter_mut().find(|b| b.trait_name == sup.trait_name) {
                    Some(seen) => {
                        for pair in sup.assoc {
                            if !seen.assoc.iter().any(|(name, _)| *name == pair.0) {
                                seen.assoc.push(pair);
                            }
                        }
                    }
                    None => bounds.push(sup),
                }
            }
        }
        bounds
    }

    /// The signature of `method` on `trait_name` or on the nearest supertrait declaring it.
    pub(crate) fn trait_member(&self, trait_name: &str, method: &str) -> Option<&TraitMethodSig> {
        self.trait_closure(trait_name)
            .iter()
            .find_map(|name| self.traits.get(name)?.methods.get(method))
    }

    /// Resolve one position of a trait's declared signature. A `Self` or associated-type
    /// position has no answer at the declaration, so it carries no information rather
    /// than a wrong one; every other position resolves and reports as usual.
    fn resolve_trait_sig_type(&mut self, ty: &ast_types::Type) -> Type {
        let mut named = Vec::new();
        collect_self_assoc(ty, &mut named);
        if !named.is_empty() || names_self(ty) {
            return Type::Unknown;
        }
        self.resolve_type(ty).unwrap_or(Type::Unknown)
    }

    /// A generic trait method's signature with `Self` and each of `assoc_types` left open,
    /// as [`Type::Generic`] placeholders named `Self` and `Self::Item`.
    ///
    /// The type standing for them can itself be a parameter spelled like one of the
    /// method's own (a caller's `U`, an impl's `T`), and substitution is by name. Holding
    /// them open lets one simultaneous substitution fill both without capture.
    pub(crate) fn open_trait_signature(
        &mut self,
        decl: &TraitMethod,
        assoc_types: &[String],
    ) -> GenericFnSig {
        self.with_open_self(assoc_types, |this| {
            let scope = this.enter_method_generics(&decl.generics, &decl.lifetimes);
            let params: Vec<Type> = decl
                .params
                .iter()
                .map(|p| this.resolve_type(&p.ty).unwrap_or(Type::Unknown))
                .collect();
            let ret = decl
                .return_type
                .as_ref()
                .map(|t| this.resolve_type(t).unwrap_or(Type::Unknown))
                .unwrap_or(Type::Void);
            let own = this.method_generics(&decl.generics, &decl.where_predicates);
            this.exit_method_generics(scope);
            own.signature(params, ret)
        })
    }

    /// Run `body` with `Self` and each of `assoc_types` resolving to their placeholders.
    fn with_open_self<R>(
        &mut self,
        assoc_types: &[String],
        body: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let placeholders = assoc_types
            .iter()
            .map(|name| {
                let open = Type::Generic(format!("{SELF_ASSOC_PREFIX}{name}"));
                (name.clone(), open)
            })
            .collect();
        let saved_assoc = std::mem::replace(&mut self.self_assoc, placeholders);
        let saved_self = self
            .self_type
            .replace(Type::Generic(SELF_TYPE_NAME.to_string()));
        let result = body(self);
        self.self_type = saved_self;
        self.self_assoc = saved_assoc;
        result
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
        let Some(info) = self.traits.get(trait_name).cloned() else {
            self.record_error(TypeError::UnknownTrait {
                trait_name: trait_name.to_string(),
                span: def.trait_name.as_ref().map(|t| t.span).unwrap_or(def.span),
            });
            return;
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
            let Some(sig) = info.methods.get(&method.name.name) else {
                errors.push(TypeError::NotATraitMethod {
                    trait_name: trait_name.to_string(),
                    method: method.name.name.clone(),
                    span: method.name.span,
                });
                continue;
            };
            if let Some(detail) = self.trait_signature_mismatch(method, sig, &info.assoc_types) {
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
    /// scope (the trait's signature as written, not as registered) so an associated-type
    /// position is compared as the type this impl bound it to, and an impl may spell that
    /// position either way.
    pub(super) fn trait_signature_mismatch(
        &mut self,
        method: &ast_types::MethodDef,
        sig: &TraitMethodSig,
        assoc_types: &[String],
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
        if method.generics.len() != sig.decl.generics.len() {
            return Some(format!(
                "declares {} type parameter(s), trait declares {}",
                method.generics.len(),
                sig.decl.generics.len()
            ));
        }
        let scope = self.enter_method_generics(&method.generics, &method.lifetimes);
        let mismatch = self.trait_types_mismatch(method, sig, assoc_types);
        self.exit_method_generics(scope);
        mismatch
    }

    /// The comparison of [`Self::trait_signature_mismatch`] past its counts, with the impl
    /// method's own parameters in scope.
    fn trait_types_mismatch(
        &mut self,
        method: &ast_types::MethodDef,
        sig: &TraitMethodSig,
        assoc_types: &[String],
    ) -> Option<String> {
        let (expected, expected_ret) = match self.expected_trait_types(method, sig, assoc_types) {
            Ok(types) => types,
            Err(detail) => return Some(detail),
        };
        for (p, expected) in method.params.iter().zip(expected.iter()) {
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
        if !ret.is_compatible_with(&expected_ret) {
            return Some(format!(
                "return type is {}, trait requires {}",
                ret, expected_ret
            ));
        }
        None
    }

    /// The parameter and return types `method` must have to implement `sig`. A generic
    /// trait method's are written in the impl's names for its parameters, after the two
    /// lists are checked to agree up to renaming; `Err` says where they do not.
    fn expected_trait_types(
        &mut self,
        method: &ast_types::MethodDef,
        sig: &TraitMethodSig,
        assoc_types: &[String],
    ) -> Result<(Vec<Type>, Type), String> {
        if !declares_generics(&sig.decl) {
            let params = sig
                .decl
                .params
                .iter()
                .map(|p| self.resolve_type(&p.ty).unwrap_or(Type::Unknown))
                .collect();
            let ret = sig
                .decl
                .return_type
                .as_ref()
                .map(|t| self.resolve_type(t).unwrap_or(Type::Unknown))
                .unwrap_or(Type::Void);
            return Ok((params, ret));
        }
        let span = method.name.span;
        let open = self.open_trait_signature(&sig.decl, assoc_types);
        let own = self.method_generics(&method.generics, &method.where_predicates);
        let mut subst: HashMap<String, Type> = self
            .self_assoc
            .iter()
            .map(|(name, ty)| (format!("{SELF_ASSOC_PREFIX}{name}"), ty.clone()))
            .collect();
        subst.extend(
            self.self_type
                .clone()
                .map(|ty| (SELF_TYPE_NAME.to_string(), ty)),
        );
        for (declared, written) in open.param_names.iter().zip(&own.param_names) {
            subst.insert(declared.clone(), Type::Generic(written.clone()));
        }
        for (declared, written) in open.param_names.iter().zip(&own.param_names) {
            if open.const_types.get(declared) != own.const_types.get(written) {
                return Err(format!(
                    "type parameter '{written}' differs in kind from the trait's '{declared}'"
                ));
            }
            let mut expected = open.bounds.get(declared).cloned().unwrap_or_default();
            for (_, ty) in expected.iter_mut().flat_map(|b| b.assoc.iter_mut()) {
                *ty = self.substitute_generic(ty, &subst, span);
            }
            let got = own.bounds.get(written).map_or(&[][..], Vec::as_slice);
            if !same_bounds(&expected, got) {
                return Err(format!(
                    "type parameter '{written}' has different bounds from the trait's '{declared}'"
                ));
            }
        }
        let same_where = open.where_predicates.len() == own.where_predicates.len()
            && open
                .where_predicates
                .iter()
                .zip(&own.where_predicates)
                .all(|(a, b)| same_predicate(a, b, &subst));
        if !same_where {
            return Err("its `where` predicates differ from the trait's".to_string());
        }
        let params = open
            .params
            .iter()
            .map(|p| self.substitute_generic(p, &subst, span))
            .collect();
        let ret = self.substitute_generic(&open.ret, &subst, span);
        Ok((params, ret))
    }
}

/// Whether a trait method declares parameters of its own, lifetimes included, so its
/// signature resolves only with them in scope.
pub(crate) fn declares_generics(method: &TraitMethod) -> bool {
    !method.generics.is_empty() || !method.lifetimes.is_empty()
}

/// Every type a trait method's signature writes: parameters, return type, and the
/// `Trait<Assoc = T>` bindings on its own parameters.
pub(crate) fn signature_types(method: &TraitMethod) -> impl Iterator<Item = &ast_types::Type> {
    let bound_types = method
        .generics
        .iter()
        .flat_map(|g| &g.bounds)
        .flat_map(|b| &b.assoc_bindings)
        .map(|(_, ty)| ty);
    method
        .params
        .iter()
        .map(|p| &p.ty)
        .chain(&method.return_type)
        .chain(bound_types)
}

/// Two bound lists are the same set of traits with the same associated-type constraints,
/// in any order: `A + B` and `B + A` promise one thing.
fn same_bounds(a: &[BoundInfo], b: &[BoundInfo]) -> bool {
    a.len() == b.len()
        && a.iter().all(|x| {
            b.iter().any(|y| {
                x.trait_name == y.trait_name
                    && x.assoc.len() == y.assoc.len()
                    && x.assoc.iter().all(|pair| y.assoc.contains(pair))
            })
        })
}

/// Collect every associated-type path an annotation names, nested positions included:
/// `Option<Self::Item>` names `Item` just as a bare `Self::Item` does.
pub(crate) fn collect_self_assoc(ty: &ast_types::Type, out: &mut Vec<Identifier>) {
    let mut names = Vec::new();
    named_types(ty, &mut names);
    out.extend(names.into_iter().filter_map(|ident| {
        ident
            .name
            .strip_prefix(SELF_ASSOC_PREFIX)
            .map(|assoc| Identifier {
                name: assoc.to_string(),
                span: ident.span,
            })
    }));
}

/// Whether an annotation names `Self` in any position, `&Self` and `Option<Self>` included.
pub(crate) fn names_self(ty: &ast_types::Type) -> bool {
    let mut names = Vec::new();
    named_types(ty, &mut names);
    names.iter().any(|ident| ident.name == SELF_TYPE_NAME)
}

fn named_types<'a>(ty: &'a ast_types::Type, out: &mut Vec<&'a Identifier>) {
    match ty {
        ast_types::Type::Named(ident) => out.push(ident),
        ast_types::Type::Reference { inner, .. } => named_types(inner, out),
        ast_types::Type::Array { element, .. } | ast_types::Type::Slice { element, .. } => {
            named_types(element, out)
        }
        ast_types::Type::Tuple { elements, .. } => {
            for element in elements {
                named_types(element, out);
            }
        }
        ast_types::Type::Generic { args, .. } => {
            for arg in args {
                if let ast_types::GenericArg::Type(inner) = arg {
                    named_types(inner, out);
                }
            }
        }
        ast_types::Type::Function { params, ret, .. } => {
            for param in params {
                named_types(param, out);
            }
            named_types(ret, out);
        }
        ast_types::Type::ImplTrait { .. }
        | ast_types::Type::DynTrait { .. }
        | ast_types::Type::Tensor { .. } => {}
    }
}
