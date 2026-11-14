//! The standard library every program is compiled with, embedded in the binary.
//!
//! Each module is Neuro source under `std/`, compiled into `neurc` with `include_str!`, so a
//! compiler binary needs no library tree beside it. The driver parses each one into the
//! source map and hands the items to module resolution, which loads them as `std::<name>`.
//! Downstream passes see ordinary declarations.
//!
//! What every module may write bare is read off the parsed prelude rather than written out a
//! second time in Rust, so `std/prelude.nr` stays the one place the prelude's contents are
//! stated.

use anyhow::{Result, anyhow};
use module_resolution::{PreludeVariant, StandardLibrary, StdModule};
use shared_types::SourceMap;
use syntax_parsing::Item;

/// Every standard-library module, as `std::<name>` and its source.
const MODULES: &[(&str, &str)] = &[("prelude", include_str!("../std/prelude.nr"))];

/// The module whose enums lend their variants to every module.
const PRELUDE: &str = "prelude";

/// Prelude enums whose variants are written with the enum's name, so a module's bare names
/// stay its own.
const QUALIFIED_ENUMS: &[&str] = &["ParseError", "Device"];

/// The prelude's traits the language itself dispatches through. They stay in scope in a file
/// written with `@no_prelude`, because a `for` loop, `@derive` and a `pool` reach them
/// whatever the file imports.
const LANGUAGE_ITEMS: &[&str] = &[
    "Copy",
    "Clone",
    "Debug",
    "Hashable",
    "Iterator",
    "IntoIterator",
    "PoolAware",
];

/// Parse every standard-library module as a file of `sources`, so a panic in one of its
/// bodies is located in that module rather than in the program's own files.
///
/// # Errors
///
/// Returns an error if a library module fails to parse, which is a compiler bug rather than
/// a fault in the program being compiled.
pub fn load(sources: &mut SourceMap) -> Result<StandardLibrary> {
    let mut modules = Vec::with_capacity(MODULES.len());
    for (name, source) in MODULES {
        // No file on disk has this name, so a diagnostic cannot be mistaken for one.
        let display = format!("<std>/{name}.nr");
        let base = sources.add(display.clone(), (*source).to_string());
        let items = syntax_parsing::parse_at(source, base).map_err(|e| {
            anyhow!(
                "internal error: the standard library module `std::{name}` failed to parse: {e}"
            )
        })?;
        modules.push(StdModule {
            name: (*name).to_string(),
            display,
            items,
        });
    }

    let variants = modules
        .iter()
        .filter(|module| module.name == PRELUDE)
        .flat_map(|module| &module.items)
        .filter_map(|item| match item {
            Item::Enum(def) if !QUALIFIED_ENUMS.contains(&def.name.name.as_str()) => Some(def),
            _ => None,
        })
        .flat_map(|def| {
            def.variants.iter().map(|variant| PreludeVariant {
                owner: def.name.name.clone(),
                variant: variant.name.name.clone(),
            })
        })
        .collect();

    Ok(StandardLibrary {
        modules,
        variants,
        language_items: LANGUAGE_ITEMS.iter().map(|name| name.to_string()).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binds_every_unqualified_prelude_enum_variant() {
        let library = load(&mut SourceMap::default()).expect("the library parses");
        let bound: Vec<String> = library
            .variants
            .iter()
            .map(|entry| format!("{}::{}", entry.owner, entry.variant))
            .collect();
        assert!(bound.contains(&"Option::Some".to_string()));
        assert!(bound.contains(&"Option::None".to_string()));
        assert!(bound.contains(&"Result::Ok".to_string()));
        assert!(bound.contains(&"Result::Err".to_string()));
        assert!(!bound.contains(&"ParseError::Empty".to_string()));
    }

    /// A language item renamed in the prelude would silently vanish from every
    /// `@no_prelude` file, so each one must still be declared there.
    #[test]
    fn every_language_item_is_a_prelude_trait() {
        let library = load(&mut SourceMap::default()).expect("the library parses");
        let prelude = library
            .modules
            .iter()
            .find(|module| module.name == PRELUDE)
            .expect("the prelude is a library module");
        for name in LANGUAGE_ITEMS {
            assert!(
                prelude
                    .items
                    .iter()
                    .any(|item| matches!(item, Item::Trait(def) if def.name.name == *name)),
                "`{name}` is not a trait of std::prelude"
            );
        }
    }
}
