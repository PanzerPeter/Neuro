# Module Resolution

**Crate**: `compiler/module-resolution`
**Entry Point**: `pub fn resolve_program(root: &Path, parse_module: &ParseModule<'_>, prelude: &[PreludeVariant]) -> Result<ResolvedProgram, ModuleError>`

Module resolution expands a root `.nr` file into the one item list the rest of the compiler sees. Read this page when you change imports, visibility or module paths. `ParseModule` is a callback that takes a module's source and the path that names it in a diagnostic.

## Overview

Resolution loads every module that a qualified path or an `import` reaches, enforces what each module exports, binds the implicit prelude's variant names, and then erases the qualifiers. Semantic analysis, HIR lowering and both backends see one flat program and never learn that modules exist.

`neurc` runs it right after parsing the root file. It prepends the prelude's declarations to the result, unless the root file opts out with `@no_prelude`.

## Architecture

- **Dependencies**: `ast-types` (the tree it walks and rewrites), `shared-types`, `thiserror`. It depends on no feature slice.
- **The parser is injected.** The caller passes in `syntax_parsing::parse`, so this slice never imports a feature slice. `neurc` is the one place the parser, the prelude source and this slice meet. The unit tests use a stub parser. A returned error is already a rendered diagnostic, and `ModuleError::Parse` prints it unchanged.
- **Public API**: `resolve_program`, `ParseModule`, `ResolvedProgram`, `ResolvedModule`, `PreludeVariant`, `ModuleError`.
- **Passes**: load the module graph, check for name collisions, resolve imports into one scope per module, then strip qualifiers. The files are [`loader.rs`](../../../compiler/module-resolution/src/loader.rs) (discovery and the module graph), [`imports.rs`](../../../compiler/module-resolution/src/imports.rs) (import scopes, re-exports and variant imports), [`walk.rs`](../../../compiler/module-resolution/src/walk.rs) (the one traversal that discovery and rewriting share) and [`rewriter.rs`](../../../compiler/module-resolution/src/rewriter.rs).

## Behavior

### Discovery

Discovery follows references. `math::sqrt` loads `math.nr` or `math/mod.nr` beside the referencing file, and an `import` names the modules it loads. Nothing globs a directory, so unrelated programs beside a file never join its build. An import path is a discovery chain like any other, and each `{...}` entry extends it. So `import ./utils::{io}` may load `utils/io` when `io` is a child module rather than an item.

A leaf module has no children. `math.nr` is a file, so `math::io::read` cannot reach an `io.nr` beside it. Only a directory with a `mod.nr` opens a level. A directory named in a path but missing its `mod.nr` is an error, not a silent miss.

Descent stops; it does not fail. A segment that names no module ends the descent, and the rest is left for the type checker. That is how `Point::new` and `Option::Some` pass through untouched. Only a path of three or more segments whose head names no module is an error. A two-segment path such as `nope::b()` reaches the type checker, which reports `unknown type 'nope' in path expression 'nope::b'`.

### Inline modules

An inline `module Name { ... }` block is a module with no file. The loader lifts it out of its parent and registers it as an ordinary graph module, so visibility, the merge and the collision check apply unchanged. Blocks nest. A block resolves paths against its file's directory, so `import ./utils` means the same inside a block or beside it. A block holds no file children, so a path through one stops where it stops for `math.nr`. A block wins over a same-named file beside it. Two blocks in one module may not share a name.

The file that declares a block sits outside it, so reaching in takes `export`. A block sees its own children and its file's sibling files, but not the file's other blocks. An import that asks for a block it cannot see is reported as such, rather than as an unknown enum.

### Visibility

A declaration is private to its module unless it is written with `export`. This pass decides it, because it is the last one that knows both the declaring file and the referencing file. A qualified path and an import both go through one visibility check. In `mod::Type::member`, the check applies to `Type`, since a method or variant carries the visibility of its type.

Field visibility is the exception. Checking `c.timeout` needs the receiver's type, and that is known only after this pass. So each merged item carries the module it came from (`ast_types::ModuleId`), and the type checker compares that with the module of the access.

### Re-exports

`export import` lets a module offer a name that it does not declare. The importing module records where the declaration really lives, so `facade::Config` resolves straight to `internal`'s `Config`, and any `as` rename is undone along the way. Only an item can be re-exported. A module or an enum variant is reached through something else, so `export import` on one is an error. Re-exports settle to a fixpoint before import scopes are built, since a chain resolves one link per round.

### Flat namespace

The merged program is one namespace. Qualifiers are verified against the owning module and then stripped. The cost is that two modules cannot declare the same name, even both keeping it private. The collision is reported, naming both files, and `export` does not lift it, because visibility says who may reach a name, not which names may coexist.

Spans are not per module either. Merged modules share one offset space, the same approximation the prepended prelude already makes. A runtime panic in a non-root module therefore reports the root file's coordinates. A compile-time error in a program of more than one module prints its message with no location, because a span from an imported module would point at the wrong line of the root file.

### Prelude

Each module's scope starts with the variants the caller supplies (`Some`, `None`, `Ok`, `Err`). They are bound after the module's own imports, and a name the module already binds or declares is skipped without an error. The prelude is therefore the weakest binding: an explicit import or a local declaration of `Some` keeps its meaning.

`@no_prelude` on a file gives that file none of the variant bindings. The prelude's declarations are a property of the whole program, since the namespace is flat. So the root file's `@no_prelude` decides whether they are prepended at all, and it is reported back as `ResolvedProgram.no_prelude`.

### Imports bind names

Each file's declarations become one import scope. It holds module aliases, item renames and variant bindings, and the rewriter consults the scope of the module it is walking. This is the one place where two modules may bind the same name to different things. A single module may not bind one name twice.

An unrenamed item import is an identity binding, since the flat namespace already makes the name reachable. What the import adds is the module load, a check that the name exists where it was taken from, and the rename forms. An import written `Shape::{Circle}` is read as a variant import only when no module is named `Shape` and an enum `Shape` is declared somewhere in the program. Otherwise it is `UnknownImportHead`.

### Renamed imports and locals

The rewriter does not track locals. A name bound by an `as` rename is replaced wherever it appears, including where a local of the same name is in scope. After `import ./other::{helper as h}`, the program `val h = 5; return h` is a type error. An unrenamed import does not have this problem, because it is an identity binding and a local of the same name shadows it. To avoid the clash, rename the import.

## Diagnostics

`neurc check` prints a `ModuleError` as `Error: Module error: ...` and exits 1. Each message names the file that holds the bad reference. Messages a user hits most often:

| Situation | Message |
|---|---|
| Reaching an item with no `export` | ``secret` is private to module `math` and cannot be used from `main.nr`; write `export` before its declaration to make it module-public`` |
| A directory in a path with no `mod.nr` | ``dir` is a directory with no `mod.nr`, so it is not a module (referenced from `main.nr`); add `dir/mod.nr` or point the path at a `.nr` file`` |
| A path of three segments naming no module | ``nope::a::b` does not name a module (referenced from `main.nr`); expected `nope.nr` or `nope/mod.nr` beside it`` |
| An import of a name the module lacks | ``module `math` declares no item named `nope` (referenced from `main.nr`)`` |
| The same name in two loaded modules | ``helper` is declared in both `helper.nr` and `other.nr`; module items share one namespace, so the two declarations collide; rename one of them`` |
| An import head that is neither a module nor an enum | ``Shape` in `main.nr` names neither a module nor an enum, so the import brings nothing into scope`` |
| `export import` of a module or variant | ``export import` in `main.nr` would re-export `x`, which names a module rather than an item; only an item can be re-exported; drop the `export``` |
| A path into an inline block from a sibling block | ``leaf::Thing` in `main.nr` cannot reach the inline module `leaf`: a block sees its own children and its file's sibling files, not the file's other blocks`` |

## Testing

- [`compiler/module-resolution/src/tests.rs`](../../../compiler/module-resolution/src/tests.rs) holds the unit tests. They write small module trees to a temporary directory and use a stub parser.
- The end-to-end module programs are in `compiler/neurc/tests/suite/`, in `modules.rs`, `imports.rs`, `inline_modules.rs`, `visibility.rs` and `implicit_prelude.rs`.

## Source

- [`compiler/module-resolution/src/lib.rs`](../../../compiler/module-resolution/src/lib.rs)
- [`compiler/module-resolution/CONTEXT.md`](../../../compiler/module-resolution/CONTEXT.md): the reasoning behind discovery, the flat merge, re-export settling and the prelude

## See Also

- [Modules language reference](../../language-reference/modules.md)
- [Syntax Parsing](syntax-parsing.md)
- [Semantic Analysis](semantic-analysis.md)
