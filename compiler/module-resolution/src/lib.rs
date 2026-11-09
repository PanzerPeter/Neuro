//! Resolve a root `.nr` file into the single item list its program is built from.
//!
//! Each `.nr` file is a module; a directory holding a `mod.nr` is a module with children.
//! A module is loaded when an `import` names it or a qualified path reaches into it
//! (`math::sqrt`, `utils::io::read`, `geometry::Point`), so a directory full of unrelated
//! programs never drags its neighbours into a build.
//!
//! Every module is a namespace. A name is resolved against the scope it is written in
//! (the locals around it, the module's own declarations and imports, an enclosing
//! module's for an inline block, then the prelude) and a path that starts with none of
//! those starts at the program's root directory. Each reference is then rewritten to the
//! canonical name of the declaration it reaches: `geometry::Point` for a `Point` declared
//! in `geometry.nr`, bare `Point` for one in the root file. Downstream slices therefore see
//! one item list of distinct names and know nothing about modules.
//!
//! A declaration is private to its module unless it is written with `export`, and that
//! rule is enforced here: this pass is the only one that still knows which file a name was
//! written in and which file it is reached from. Field visibility needs the receiver's
//! type, so each item carries the module it came from and the type checker settles it.
//!
//! An inline `module Name { ... }` block is a module with no file of its own and is loaded
//! as one, so every rule above reaches it unchanged. `export import` is the one way a name
//! reaches *through* a module it was not declared in: the importing module records where
//! the declaration really lives, and a qualified path lands on it in a single step.
//!
//! Every module also starts with the implicit prelude's variant bindings, so `Some` and
//! `Ok` read as themselves without an import. They are the weakest binding there is: an
//! explicit import of the same name wins, and so does a local declaration of it. A file
//! written with `@no_prelude` gets none of them.

use std::path::Path;

mod defaults;
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
    /// Whether the root file was written with `@no_prelude`. The prelude's declarations
    /// join the program's one item list, so they are a property of the program rather
    /// than of any one file: the root is the file that decides for it.
    pub no_prelude: bool,
}

/// One enum variant the implicit prelude binds in every module that did not opt out.
///
/// The prelude's contents belong to the driver, which owns the prelude source; this slice
/// is told what to bind for the same reason it is handed a parser: it may not reach into
/// another slice to find out.
#[derive(Debug, Clone)]
pub struct PreludeVariant {
    /// The enum the variant belongs to, by its canonical name.
    pub owner: String,
    /// The variant's own name, which is also the bare name a module may write.
    pub variant: String,
}

/// The parser the caller injects: called with a module's source and the path that names
/// the module in a diagnostic, it returns the module's items or its rendered error.
pub type ParseModule<'a> = dyn Fn(&str, &str) -> Result<Vec<Item>, String> + 'a;

/// Everything that can go wrong turning a root file into one program.
#[derive(Debug, thiserror::Error)]
pub enum ModuleError {
    #[error("failed to read module file `{path}`: {message}")]
    Read { path: String, message: String },

    /// `message` is the injected parser's own rendering, which it was given the module's
    /// path to write, so it is reported as-is.
    #[error("{message}")]
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
        "`{path}` would name both `{first}` and `{second}`; rename the inline module or \
         the file so the two modules have paths of their own"
    )]
    DuplicateModulePath {
        path: String,
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
        "`{name}` is imported in `{from}`, which declares a `{name}` of its own; one name \
         can stand for one thing, so rename the import with `as`"
    )]
    ImportShadowsDeclaration { name: String, from: String },

    #[error(
        "`{name}` is declared in `{root}`, not in `{from}`, and nothing in `{from}` imports \
         it; every module is a namespace, so import it or write its path"
    )]
    NotInScope {
        name: String,
        from: String,
        root: String,
    },

    #[error(
        "variant `{variant}` is used without its enum in `{from}` but no import brings it \
         into scope; write `Enum::{variant}` or add `import Enum::{{{variant}}}`"
    )]
    UnimportedVariant { variant: String, from: String },

    #[error(
        "`{item}` is private to module `{module}` and cannot be used from `{from}`; \
         write `export` before its declaration to make it module-public"
    )]
    PrivateItem {
        module: String,
        item: String,
        from: String,
    },

    #[error(
        "`{name}` is declared twice as an inline module in `{from}`; one module name can \
         stand for one block; rename one of them"
    )]
    DuplicateInlineModule { name: String, from: String },

    #[error(
        "`export import` in `{from}` would re-export `{name}`, which names {what} rather \
         than an item; only an item can be re-exported; drop the `export`"
    )]
    ExportImportNotItem {
        name: String,
        what: String,
        from: String,
    },

    #[error(
        "`{path}` in `{from}` names neither a module nor an enum: there is no `{head}.nr`, \
         no `{head}/mod.nr`, no inline `module {head}` in scope, and no `enum {head}`, so \
         the import brings nothing into scope"
    )]
    UnknownImportHead {
        path: String,
        from: String,
        head: String,
    },

    #[error(
        "`{path}` in `{from}` cannot reach the inline module `{head}`: a block is in scope \
         inside the module that declares it, and from anywhere else is reached through that \
         module's path, or moved into a `{head}.nr` of its own"
    )]
    UnreachableInlineModule {
        path: String,
        from: String,
        head: String,
    },
}

/// Expand `root` and every module it reaches into one item list.
///
/// Parsing is supplied by the caller rather than imported: this slice depends only on the
/// AST it rewrites, so the parser stays on the driver's side of the boundary. It is
/// called with a module's source and the path to name that module by in a diagnostic.
/// `prelude`
/// names the enum variants every module may write bare (`Some`, `None`, `Ok`, `Err`)
/// and comes from the caller for the same reason.
///
/// # Errors
///
/// Returns a [`ModuleError`] when a module file cannot be read or parsed, when an import or
/// a qualified path names a module or an item that does not exist, when a name it reaches
/// for is private to the module that declares it or is not in scope where it is written,
/// when an `export import` names something other than an item, or when an import binds a
/// name its scope already holds.
pub fn resolve_program(
    root: &Path,
    parse_module: &ParseModule<'_>,
    prelude: &[PreludeVariant],
) -> Result<ResolvedProgram, ModuleError> {
    let mut graph = loader::ModuleGraph::load(root, parse_module)?;
    let scopes = imports::resolve_imports(&mut graph, prelude)?;
    rewriter::resolve_names(&mut graph, &scopes, prelude)?;
    Ok(graph.into_program())
}
