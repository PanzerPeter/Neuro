# Module Resolution

**Status**: Complete (1G)
**Crate**: `compiler/module-resolution`
**Entry Point**: `pub fn resolve_program(root: &Path, parse: &dyn Fn(&str) -> Result<Vec<Item>, String>, prelude: &[PreludeVariant]) -> Result<ResolvedProgram, ModuleError>`

## Overview

Module resolution expands a root `.nr` file into the single item list the rest of the
compiler sees. It loads every module a qualified path or an `import` reaches into,
enforces what each module exports, binds the implicit prelude's variant names, and then
**erases the qualifiers**: semantic analysis, HIR lowering, and both backends never learn
that modules exist.

`neurc` runs it immediately after parsing the root file, and prepends the prelude's
declarations to its output unless the root file opted out with `@no_prelude`.

## Architecture

- **Dependencies**: `ast-types` (the tree it walks and rewrites), `shared-types`,
  `thiserror`. It depends on **no feature slice**.
- **The parser is injected, not imported.** The caller passes `syntax_parsing::parse` in.
  `neurc` is the one place the parser, the prelude source, and this slice meet — which is
  what keeps this slice free of a cross-slice dependency. Its unit tests use a stub parser.
- **Public API**: `resolve_program`, `ResolvedProgram`, `ResolvedModule`, `PreludeVariant`,
  `ModuleError`.

## Behavior

**Discovery is reference-driven.** A module is loaded when an `import` names it or a
qualified path reaches into it: `math::sqrt` looks for `math.nr` or `math/mod.nr` beside
the referencing file. Nothing globs a directory, so unrelated files sitting next to a
program are never dragged into its build.

**A leaf module has no children.** `math.nr` is a file, so `math::io::read` cannot reach an
`io.nr` beside it — only a directory with a `mod.nr` opens a level. A directory named in a
path but missing its `mod.nr` is an error rather than a silent miss.

**An inline `module { }` block is a module with no file.** Blocks are lifted into the module
graph like files, so visibility, the flat merge, the collision check, and nesting all work
without a special case. The file declaring a block is *outside* it, so reaching in needs
`export`. A block holds no file children, and it wins over a same-named file beside it.

**Visibility is settled here.** A `func`, `struct`, `enum`, `trait`, `const`, or `newtype` is
private to its own module unless written with `export`; this pass is the last one that knows
both the declaring file and the referencing one. Field visibility is the exception — it needs
the receiver's type — so each item is stamped with the module it came from
(`ast_types::ModuleId`) and the type checker enforces field access against that stamp.

**`export import` re-exports an item.** The importing module records where the declaration
really lives, so a facade offers a flatter API than its internals and any `as` rename is
undone on the way through. Only an item can be re-exported: a module and an enum variant are
each reached through something else, so `export import` on one is an error.

**The merged namespace is flat.** Every module's items join one namespace and qualifiers are
stripped after being verified. The cost is that two modules cannot both declare `helper` —
that is a reported collision naming both files, not a silent winner, and `export` does not
lift it: visibility says who may reach a name, not which names may coexist.

**The prelude is the weakest binding there is.** Each module's scope is seeded with the
variant names the caller supplies (`Some`, `None`, `Ok`, `Err`) *after* its own imports, and a
name the module already bound or declares itself is skipped rather than reported — which is
what makes shadowing the prelude a non-error. `@no_prelude` marks one file; on the root file
it drops the prelude's declarations from the whole program, since the namespace is flat.

**Descent stops, it does not fail.** A segment naming no module ends the descent and leaves
the remainder to the type checker — that is how `Point::new` and `Option::Some` pass through
untouched. Only a path of three or more segments whose head resolves to nothing is an error.

## Known Limitations

- Spans stay per-file, and merged modules share one offset space, so a runtime diagnostic
  from a non-root module reports a position in the root file's coordinates. Per-file
  attribution needs a multi-source backend contract.
- Rewriting does not track locals: a bare name bound by an import is replaced whether or not
  a local of the same name is in scope. Rename the import with `as` instead of shadowing it.

## Resources

- [module-resolution CONTEXT](../../../compiler/module-resolution/CONTEXT.md) — slice contract
- [Modules language reference](../../language-reference/modules.md)
