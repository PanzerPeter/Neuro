//! The implicit standard library every program is compiled with.
//!
//! The driver parses [`PRELUDE_SOURCE`] and prepends its items to the program's own.
//! Downstream passes see ordinary declarations — nothing about `Option` or `Result` is
//! special-cased in the type checker, the lowering, or the backend.
//!
//! The prelude also binds its enums' variants in every module, so `Some` and `Ok` read as
//! themselves without an import. That binding is applied by module resolution, which is
//! told what to bind from here: the variant list is read off the parsed prelude rather
//! than written out a second time in Rust, so `prelude.nr` stays the one place the
//! prelude's contents are stated.

use anyhow::{anyhow, Result};
use ast_types::PRELUDE_MODULE;
use module_resolution::PreludeVariant;
use syntax_parsing::Item;

/// The prelude in Neuro source form, so the declarations read exactly as a user would
/// write them and stay in one place as more items join them.
const PRELUDE_SOURCE: &str = include_str!("prelude.nr");

/// The parsed prelude: the declarations to prepend, and the variant names every module
/// may write bare.
pub struct Prelude {
    items: Vec<Item>,
    variants: Vec<PreludeVariant>,
}

/// Parse the prelude.
///
/// # Errors
///
/// Returns an error if the prelude source itself fails to parse, which is a compiler
/// bug rather than a fault in the program being compiled.
pub fn load() -> Result<Prelude> {
    let items = syntax_parsing::parse(PRELUDE_SOURCE).map_err(|e| {
        anyhow!(
            "internal error: the compiler prelude failed to parse: {}",
            e
        )
    })?;

    let variants = items
        .iter()
        .filter_map(|item| match item {
            Item::Enum(def) => Some(def),
            _ => None,
        })
        .flat_map(|def| {
            def.variants.iter().map(|variant| PreludeVariant {
                owner: def.name.name.clone(),
                variant: variant.name.name.clone(),
            })
        })
        .collect();

    Ok(Prelude { items, variants })
}

impl Prelude {
    /// The variants module resolution binds without an import: `Some`, `None`, `Ok`, `Err`.
    pub fn variants(&self) -> &[PreludeVariant] {
        &self.variants
    }

    /// Prepend the prelude declarations to a resolved program's items.
    ///
    /// A prelude item whose name the program already declares is dropped: a local
    /// declaration shadows the prelude, which is the module system's rule and the only way
    /// to keep a program that defines its own `Result` compilable. Dropping one
    /// declaration takes with it every prelude declaration written against it — the
    /// prelude's own bodies are compiled against the prelude's own types, and a
    /// replacement is a different type with a different surface.
    pub fn prepend(self, items: Vec<Item>) -> Vec<Item> {
        let declared: Vec<&str> = items.iter().filter_map(item_name).collect();
        let dropped = dropped_declarations(&declared);
        let mut combined: Vec<Item> = self
            .items
            .into_iter()
            .filter(|item| !is_dropped(item, &dropped))
            .map(stamp_prelude_module)
            .collect();
        combined.extend(items);
        combined
    }
}

/// Prelude declarations written against other prelude declarations, and the names each
/// needs. Shadowing a needed name takes the dependent declaration down with it.
const PRELUDE_DEPENDENCIES: &[(&str, &[&str])] = &[("Chars", &["Option", "Iterator"])];

/// Every prelude name a program's own declarations displace: the names it declares
/// outright, plus the prelude declarations written against those.
fn dropped_declarations(declared: &[&str]) -> Vec<String> {
    let mut dropped: Vec<String> = declared.iter().map(|name| name.to_string()).collect();
    for (dependent, needs) in PRELUDE_DEPENDENCIES {
        if needs.iter().any(|need| dropped.iter().any(|d| d == need)) {
            dropped.push((*dependent).to_string());
        }
    }
    dropped
}

/// Whether a prelude item is displaced: the declaration itself, or an `impl` block
/// extending a displaced type — those methods belong to the prelude's type, not to
/// whatever the program put in its place.
fn is_dropped(item: &Item, dropped: &[String]) -> bool {
    if let Item::Impl(def) = item {
        return dropped.iter().any(|name| name == &def.type_name.name);
    }
    item_name(item).is_some_and(|name| dropped.iter().any(|d| d == name))
}

/// Mark a prelude declaration as belonging to the prelude's own module.
fn stamp_prelude_module(mut item: Item) -> Item {
    match &mut item {
        Item::Function(def) => def.module = PRELUDE_MODULE,
        Item::Struct(def) => def.module = PRELUDE_MODULE,
        Item::Impl(def) => def.module = PRELUDE_MODULE,
        Item::Const(def) => def.module = PRELUDE_MODULE,
        Item::Enum(_)
        | Item::Newtype(_)
        | Item::Trait(_)
        | Item::Import(_)
        | Item::Module(_)
        | Item::NoPrelude(_) => {}
    }
    item
}

/// The name an item declares, for shadow detection.
fn item_name(item: &Item) -> Option<&str> {
    match item {
        Item::Function(def) => Some(&def.name.name),
        Item::Struct(def) => Some(&def.name.name),
        Item::Enum(def) => Some(&def.name.name),
        Item::Trait(def) => Some(&def.name.name),
        Item::Const(def) => Some(&def.name.name),
        Item::Newtype(def) => Some(&def.name.name),
        // An `impl` block extends a type declared elsewhere, and module resolution has
        // already consumed every import, inline block, and `@no_prelude` marker — none
        // declares a name that could shadow.
        Item::Impl(_) | Item::Import(_) | Item::Module(_) | Item::NoPrelude(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binds_every_prelude_enum_variant() {
        let prelude = load().expect("the prelude parses");
        let bound: Vec<String> = prelude
            .variants()
            .iter()
            .map(|entry| format!("{}::{}", entry.owner, entry.variant))
            .collect();
        assert!(bound.contains(&"Option::Some".to_string()));
        assert!(bound.contains(&"Option::None".to_string()));
        assert!(bound.contains(&"Result::Ok".to_string()));
        assert!(bound.contains(&"Result::Err".to_string()));
    }

    #[test]
    fn a_local_declaration_replaces_the_prelude_one() {
        let program = syntax_parsing::parse("enum Option { Nothing }").expect("program parses");
        let combined = load().expect("the prelude parses").prepend(program);
        let options = combined
            .iter()
            .filter(|item| matches!(item, Item::Enum(def) if def.name.name == "Option"))
            .count();
        assert_eq!(options, 1);
    }

    /// `Chars::next` answers `Option<char>`, so a program that brings its own `Option`
    /// leaves the prelude's iterator with nothing to return. It goes with it, along with
    /// the `impl` block that extends it.
    #[test]
    fn shadowing_a_prelude_name_withdraws_what_depends_on_it() {
        let program = syntax_parsing::parse("enum Option { Nothing }").expect("program parses");
        let combined = load().expect("the prelude parses").prepend(program);
        assert!(
            !combined
                .iter()
                .any(|item| matches!(item, Item::Struct(def) if def.name.name == "Chars")),
            "the dependent declaration is withdrawn"
        );
        assert!(
            !combined
                .iter()
                .any(|item| matches!(item, Item::Impl(def) if def.type_name.name == "Chars")),
            "and so is the impl block extending it"
        );
    }
}
