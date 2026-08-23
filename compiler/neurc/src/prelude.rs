//! The implicit standard library every program is compiled with.
//!
//! Until the module system lands there is nowhere to import a standard library from,
//! so the driver parses [`PRELUDE_SOURCE`] and prepends its items to the program's own.
//! Downstream passes see ordinary declarations — nothing about `Option` or `Result` is
//! special-cased in the type checker, the lowering, or the backend.

use anyhow::{anyhow, Result};
use syntax_parsing::Item;

/// The module id the prelude's own declarations are stamped with.
///
/// The prelude is prepended after module resolution has numbered the program's files, so
/// it needs an id no loaded module can hold. Its declarations are reachable from every
/// module the same way — a private prelude field is private to the prelude, not to
/// whichever file happens to be the root.
const PRELUDE_MODULE: ast_types::ModuleId = ast_types::ModuleId::MAX;

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
        .map(stamp_prelude_module)
        .collect();
    combined.extend(items);
    Ok(combined)
}

/// Mark a prelude declaration as belonging to the prelude's own module.
fn stamp_prelude_module(mut item: Item) -> Item {
    match &mut item {
        Item::Function(def) => def.module = PRELUDE_MODULE,
        Item::Struct(def) => def.module = PRELUDE_MODULE,
        Item::Impl(def) => def.module = PRELUDE_MODULE,
        Item::Const(def) => def.module = PRELUDE_MODULE,
        Item::Enum(_) | Item::Newtype(_) | Item::Trait(_) | Item::Import(_) | Item::Module(_) => {}
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
        // already consumed every import and inline block — none declares a name that could
        // shadow.
        Item::Impl(_) | Item::Import(_) | Item::Module(_) => None,
    }
}
