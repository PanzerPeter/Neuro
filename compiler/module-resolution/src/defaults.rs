//! Supply what a trait provides to every impl of it, whichever module each is in: its
//! default methods, and the associated types its supertraits declare.
//!
//! The parser already does this inside one file. A trait and an impl in different modules
//! meet only here, once every name is canonical: `impl shapes::Drawable for Dot` names the
//! same trait as the declaration `shapes::Drawable`, and nothing earlier could tell.

use std::collections::{HashMap, HashSet};

use ast_types::{GenericArg, ImplDef, Item, MethodDef, ModuleId, TraitMethod, Type};
use shared_types::Identifier;

/// Copy each bodied trait method into every `impl Trait for Type` that does not write it.
///
/// A default body was resolved in the trait's module, so the copy already names what the
/// trait's module meant. A method the impl writes itself overrides the default. The parser
/// leaves to this pass the defaults whose type parameters share a name with the impl's,
/// which are copied here with those parameters renamed.
///
/// The copy is checked as the trait's module (`trait_modules`, by canonical name), since a
/// field it reads or a struct it builds was written there.
pub(crate) fn inject_trait_defaults(items: &mut [Item], trait_modules: &HashMap<String, ModuleId>) {
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
        let Some((trait_name, trait_defaults)) = imp
            .trait_name
            .clone()
            .and_then(|name| defaults.get(&name.name).map(|found| (name, found)))
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
            let mut copy = MethodDef {
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
                module: trait_modules.get(&trait_name.name).copied(),
                span: method.span,
            };
            // `func f<T>` provided to `impl<T> Tr for C<T>`: the method's `T` is renamed so
            // the one name does not mean two parameters in the impl.
            crate::rename::rename_clashing_generics(&mut copy, &imp.generics);
            imp.methods.push(copy);
        }
    }
}

/// Give each impl of a subtrait the associated types its supertraits declare, as the same
/// type's impl of the declaring supertrait binds them (`ImplDef::inherited_assoc`).
///
/// A name the subtrait declares itself is its own, and a name two different supertraits
/// declare is neither's: both stay out. The supertrait impl's binding is renamed into the
/// subtrait impl's type-parameter names, position by position, so `impl<U> Sub for W<U>`
/// reads `Item = U` from `impl<T> Src for W<T> { type Item = T }`. When no impl, or more
/// than one, fits, nothing is inherited, and the checker reports the missing supertrait impl.
pub(crate) fn inherit_assoc_types(items: &mut [Item]) {
    let mut traits: HashMap<&str, (Vec<String>, Vec<String>)> = HashMap::new();
    for item in items.iter() {
        if let Item::Trait(def) = item {
            let own = def.assoc_types.iter().map(|a| a.name.clone()).collect();
            let sups = def
                .supertraits
                .iter()
                .map(|b| b.trait_name.name.clone())
                .collect();
            traits.insert(def.name.name.as_str(), (own, sups));
        }
    }
    let mut inherited: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for (&name, (own, _)) in &traits {
        let mut owners: HashMap<&str, HashSet<&str>> = HashMap::new();
        let mut seen: HashSet<&str> = HashSet::from([name]);
        let mut queue: Vec<&str> = vec![name];
        while let Some(next) = queue.pop() {
            let Some((_, sups)) = traits.get(next) else {
                continue;
            };
            for sup in sups {
                if !seen.insert(sup.as_str()) {
                    continue;
                }
                queue.push(sup.as_str());
                for assoc in traits.get(sup.as_str()).into_iter().flat_map(|t| &t.0) {
                    if !own.contains(assoc) {
                        owners.entry(assoc).or_default().insert(sup.as_str());
                    }
                }
            }
        }
        let unique: Vec<(String, String)> = owners
            .into_iter()
            .filter(|(_, by)| by.len() == 1)
            .map(|(assoc, by)| {
                (
                    assoc.to_string(),
                    by.into_iter().next().unwrap().to_string(),
                )
            })
            .collect();
        if !unique.is_empty() {
            inherited.insert(name.to_string(), unique);
        }
    }
    if inherited.is_empty() {
        return;
    }
    let impls: Vec<ImplDef> = items
        .iter()
        .filter_map(|item| match item {
            Item::Impl(imp) if imp.trait_name.is_some() => Some(imp.clone()),
            _ => None,
        })
        .collect();
    for item in items.iter_mut() {
        let Item::Impl(imp) = item else { continue };
        let Some(names) = imp.trait_name.as_ref().and_then(|t| inherited.get(&t.name)) else {
            continue;
        };
        let mut names = names.clone();
        names.sort();
        for (assoc, owner) in names {
            let mut found = impls.iter().filter_map(|sup| {
                if sup.trait_name.as_ref().map(|t| t.name.as_str()) != Some(owner.as_str())
                    || sup.type_name.name != imp.type_name.name
                {
                    return None;
                }
                let renames = positional_renames(sup, imp)?;
                let (_, ty) = sup.assoc_types.iter().find(|(n, _)| n.name == assoc)?;
                let mut ty = ty.clone();
                crate::rename::rename_type(&mut ty, renames);
                Some(ty)
            });
            if let (Some(ty), None) = (found.next(), found.next()) {
                let name = Identifier {
                    name: assoc,
                    span: imp.type_name.span,
                };
                imp.inherited_assoc.push((name, ty));
            }
        }
    }
}

/// How `sup`'s type parameters are spelled in `imp`, when `sup` covers the type `imp` is for:
/// each of `sup`'s type arguments is a parameter, renamed to `imp`'s argument in its
/// position, or the same type `imp` writes there.
fn positional_renames(sup: &ImplDef, imp: &ImplDef) -> Option<HashMap<String, String>> {
    if sup.type_args.len() != imp.type_args.len() {
        return None;
    }
    let mut renames = HashMap::new();
    for (theirs, ours) in sup.type_args.iter().zip(&imp.type_args) {
        if let Type::Named(param) = theirs
            && sup.generics.iter().any(|g| g.name.name == param.name)
        {
            let Type::Named(spelled) = ours else {
                return None;
            };
            let prior = renames.insert(param.name.clone(), spelled.name.clone());
            if prior.is_some_and(|p| p != spelled.name) {
                return None;
            }
        } else if !same_type(theirs, ours) {
            return None;
        }
    }
    Some(renames)
}

/// Whether two written types are the same, spans aside. A form this does not walk is never
/// the same, which leaves an impl over it without inherited types rather than with wrong ones.
fn same_type(a: &Type, b: &Type) -> bool {
    match (a, b) {
        (Type::Named(x), Type::Named(y)) => x.name == y.name,
        (
            Type::Generic {
                name: x, args: xs, ..
            },
            Type::Generic {
                name: y, args: ys, ..
            },
        ) => {
            x.name == y.name
                && xs.len() == ys.len()
                && xs.iter().zip(ys).all(|pair| match pair {
                    (GenericArg::Type(p), GenericArg::Type(q)) => same_type(p, q),
                    _ => false,
                })
        }
        (
            Type::Reference {
                inner: x,
                mutable: mx,
                ..
            },
            Type::Reference {
                inner: y,
                mutable: my,
                ..
            },
        ) => mx == my && same_type(x, y),
        (Type::Slice { element: x, .. }, Type::Slice { element: y, .. }) => same_type(x, y),
        (Type::Tuple { elements: xs, .. }, Type::Tuple { elements: ys, .. }) => {
            xs.len() == ys.len() && xs.iter().zip(ys).all(|(p, q)| same_type(p, q))
        }
        _ => false,
    }
}
