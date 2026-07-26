//! The implicit standard library every program is compiled with.
//!
//! Until the module system lands there is nowhere to import a standard library from,
//! so the driver parses [`PRELUDE_SOURCE`] and prepends its items to the program's own.
//! Downstream passes see ordinary declarations — nothing about `Option` or `Result` is
//! special-cased in the type checker, the lowering, or the backend.

use anyhow::{anyhow, Result};
use syntax_parsing::Item;

/// The prelude in Neuro source form, so the declarations read exactly as a user would
/// write them and stay in one place as more items join them.
const PRELUDE_SOURCE: &str = include_str!("prelude.nr");

/// Prepend the prelude declarations to a parsed program.
///
/// A prelude item whose name the program already declares is dropped: a local
/// declaration shadows the prelude, which is both the module system's eventual rule and
/// the only way to keep a program that defines its own `Result` compilable.
///
/// # Errors
///
/// Returns an error if the prelude source itself fails to parse, which is a compiler
/// bug rather than a fault in the program being compiled.
pub fn with_prelude(items: Vec<Item>) -> Result<Vec<Item>> {
    let prelude = syntax_parsing::parse(PRELUDE_SOURCE).map_err(|e| {
        anyhow!(
            "internal error: the compiler prelude failed to parse: {}",
            e
        )
    })?;

    let declared: Vec<&str> = items.iter().filter_map(item_name).collect();
    let mut combined: Vec<Item> = prelude
        .into_iter()
        .filter(|item| !item_name(item).is_some_and(|name| declared.contains(&name)))
        .collect();
    combined.extend(items);
    Ok(combined)
}

/// The name an item declares, for shadow detection. An `impl` block declares no name of
/// its own — it extends a type declared elsewhere — so it never shadows a prelude item.
fn item_name(item: &Item) -> Option<&str> {
    match item {
        Item::Function(def) => Some(&def.name.name),
        Item::Struct(def) => Some(&def.name.name),
        Item::Enum(def) => Some(&def.name.name),
        Item::Trait(def) => Some(&def.name.name),
        Item::Const(def) => Some(&def.name.name),
        Item::Newtype(def) => Some(&def.name.name),
        Item::Impl(_) => None,
    }
}
