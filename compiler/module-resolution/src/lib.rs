//! Resolve a root `.nr` file into the single item list its program is built from.
//!
//! Each `.nr` file is a module; a directory holding a `mod.nr` is a module with children.
//! A module is loaded when an `import` names it or a qualified path reaches into it —
//! `math::sqrt`, `utils::io::read`, `geometry::Point` — so a directory full of unrelated
//! programs never drags its neighbours into a build.
//!
//! Resolution flattens: every loaded module's items are merged into one namespace, the
//! qualifier on each path is verified against the module that owns the name and then
//! stripped, and every name an import bound is replaced by what it stands for. Downstream
//! slices therefore see an ordinary single-file program and know nothing about modules.
//! The flat namespace is what makes per-module privacy impossible this phase: two modules
//! cannot each declare `helper`. That collision is reported rather than silently resolved,
//! and lifting it is the visibility item's job.

use std::path::Path;

mod imports;
mod loader;
mod rewriter;
mod walk;

#[cfg(test)]
mod tests;

use ast_types::Item;

pub use loader::ResolvedModule;

/// The merged program a root file expands to.
#[derive(Debug, Clone)]
pub struct ResolvedProgram {
    /// Every loaded module's items, root module first, in load order.
    pub items: Vec<Item>,
    /// One entry per loaded module, for diagnostics and driver reporting.
    pub modules: Vec<ResolvedModule>,
}

/// Everything that can go wrong turning a root file into one program.
#[derive(Debug, thiserror::Error)]
pub enum ModuleError {
    #[error("failed to read module file `{path}`: {message}")]
    Read { path: String, message: String },

    #[error("failed to parse module `{path}`: {message}")]
    Parse { path: String, message: String },

    #[error(
        "`{name}` is a directory with no `mod.nr`, so it is not a module (referenced from `{from}`); \
         add `{expected}` or point the path at a `.nr` file"
    )]
    MissingModFile {
        name: String,
        from: String,
        expected: String,
    },

    #[error("module `{module}` declares no item named `{item}` (referenced from `{from}`)")]
    UndeclaredItem {
        module: String,
        item: String,
        from: String,
    },

    #[error(
        "`{path}` does not name a module (referenced from `{from}`); \
         expected `{head}.nr` or `{head}/mod.nr` beside it"
    )]
    UnknownModule {
        path: String,
        from: String,
        head: String,
    },

    #[error(
        "`{name}` is declared in both `{first}` and `{second}`; module items share one \
         namespace, so the two declarations collide — rename one of them"
    )]
    DuplicateItem {
        name: String,
        first: String,
        second: String,
    },

    #[error(
        "qualified path `{path}` has too many segments (referenced from `{from}`); \
         expected `module::item`, `module::Type::member`, or `module::Enum::Variant`"
    )]
    PathTooDeep { path: String, from: String },

    #[error(
        "`{name}` is imported twice in `{from}`; one name can stand for one thing, \
         so rename one of them with `as`"
    )]
    DuplicateImport { name: String, from: String },

    #[error(
        "variant `{variant}` is used without its enum in `{from}` but no import brings it \
         into scope; write `Enum::{variant}` or add `import Enum::{{{variant}}}`"
    )]
    UnimportedVariant { variant: String, from: String },
}

/// Expand `root` and every module it reaches into one item list.
///
/// Parsing is supplied by the caller rather than imported: this slice depends only on the
/// AST it rewrites, so the parser stays on the driver's side of the boundary.
///
/// # Errors
///
/// Returns a [`ModuleError`] when a module file cannot be read or parsed, when an import or
/// a qualified path names a module or an item that does not exist, or when two modules
/// declare the same name.
pub fn resolve_program(
    root: &Path,
    parse_module: &dyn Fn(&str) -> Result<Vec<Item>, String>,
) -> Result<ResolvedProgram, ModuleError> {
    let mut graph = loader::ModuleGraph::load(root, parse_module)?;
    graph.check_name_collisions()?;
    let scopes = imports::resolve_imports(&graph)?;
    rewriter::strip_qualifiers(&mut graph, &scopes)?;
    Ok(graph.into_program())
}
