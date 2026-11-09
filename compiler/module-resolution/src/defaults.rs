//! Supply a trait's default methods to every impl of it, whichever module each is in.
//!
//! The parser already does this inside one file. A trait and an impl in different modules
//! meet only here, once every name is canonical: `impl shapes::Drawable for Dot` names the
//! same trait as the declaration `shapes::Drawable`, and nothing earlier could tell.

use std::collections::HashMap;

use ast_types::{Item, MethodDef, TraitMethod};

/// Copy each bodied trait method into every `impl Trait for Type` that does not write it.
///
/// A default body was resolved in the trait's module, so the copy already names what the
/// trait's module meant. A method the impl writes itself overrides the default.
pub(crate) fn inject_trait_defaults(items: &mut [Item]) {
    let mut defaults: HashMap<String, Vec<TraitMethod>> = HashMap::new();
    for item in items.iter() {
        if let Item::Trait(def) = item {
            let bodied: Vec<TraitMethod> = def
                .methods
                .iter()
                .filter(|method| method.default_body.is_some())
                .cloned()
                .collect();
            if !bodied.is_empty() {
                defaults.insert(def.name.name.clone(), bodied);
            }
        }
    }
    for item in items.iter_mut() {
        let Item::Impl(imp) = item else { continue };
        let Some(trait_defaults) = imp
            .trait_name
            .as_ref()
            .and_then(|name| defaults.get(&name.name))
        else {
            continue;
        };
        for method in trait_defaults {
            if imp.methods.iter().any(|m| m.name.name == method.name.name) {
                continue;
            }
            let Some(body) = &method.default_body else {
                continue;
            };
            imp.methods.push(MethodDef {
                name: method.name.clone(),
                generics: method.generics.clone(),
                lifetimes: method.lifetimes.clone(),
                where_predicates: method.where_predicates.clone(),
                self_param: method.self_param.clone(),
                params: method.params.clone(),
                return_type: method.return_type.clone(),
                return_lifetimes: 0,
                body: body.clone(),
                attributes: Vec::new(),
                span: method.span,
            });
        }
    }
}
