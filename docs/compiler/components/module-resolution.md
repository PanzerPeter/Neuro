# Module Resolution

**Crate**: `compiler/module-resolution`
**Entry Point**: `pub fn resolve_program(root: &Path, parse_module: &ParseModule<'_>, prelude: &[PreludeVariant]) -> Result<ResolvedProgram, ModuleError>`

Module resolution expands a root `.nr` file into the one item list the rest of the compiler sees. Read this page when you change imports, visibility or module paths. `ParseModule` is a callback that takes a module's source and the path that names it in a diagnostic.

## Overview

Resolution loads every module that a path or an `import` reaches, resolves each name against the scope it is written in, enforces what each module exports, and rewrites every reference to the canonical name of the declaration it reaches. A declaration in `geometry.nr` is named `geometry::Point` from then on. One in the root file keeps its bare name. Semantic analysis, HIR lowering and both backends see one program of distinct names and never learn that modules exist.

`neurc` runs it right after parsing the root file. It prepends the prelude's declarations to the result, unless the root file opts out with `@no_prelude`.

## Architecture

- **Dependencies**: `ast-types` (the tree it walks and rewrites), `shared-types`, `thiserror`. It depends on no feature slice.
- **The parser is injected.** The caller passes in `syntax_parsing::parse`, so this slice never imports a feature slice. `neurc` is the one place the parser, the prelude source and this slice meet. The unit tests use a stub parser. A returned error is already a rendered diagnostic, and `ModuleError::Parse` prints it unchanged.
- **Public API**: `resolve_program`, `ParseModule`, `ResolvedProgram`, `ResolvedModule`, `PreludeVariant`, `ModuleError`.
- **Passes**: load the module graph, resolve imports into one scope per module, rewrite every name, then rename the declarations and supply trait defaults across modules. The files are [`loader.rs`](../../../compiler/module-resolution/src/loader.rs) (discovery, module paths and the module graph), [`imports.rs`](../../../compiler/module-resolution/src/imports.rs) (import scopes, re-exports and variant imports), [`walk.rs`](../../../compiler/module-resolution/src/walk.rs) (the one traversal that discovery and rewriting share, which also tracks locals), [`rewriter.rs`](../../../compiler/module-resolution/src/rewriter.rs) (name lookup) and [`defaults.rs`](../../../compiler/module-resolution/src/defaults.rs).

## Behavior

### Module paths and canonical names

A module's path is where its file sits under the root directory, with `/` read as `::`: `utils/io.nr` is `utils::io`, and `utils/mod.nr` is `utils`. An inline block adds its name to its parent's path. A file reached through `../` keeps a `..` segment for each level it sits above the root.

Every declaration outside the root module is renamed to its path followed by its name, and every reference to it is rewritten to match. Two modules that each declare `helper` therefore reach the type checker as two names. The root's declarations stay bare, so a single-file program's diagnostics are unchanged. Derived `Debug` prints the declared name, not the canonical one.

### Name lookup

A bare name written in a module resolves to the first of these that has it:

1. a local: a parameter, a `val` or `var`, a pattern or loop binding, a closure parameter, or a generic parameter;
2. the module's imports, its own declarations and its inline blocks;
3. the same for each module around it, when it is an inline block;
4. the prelude's variants.

A name none of these has is left for the type checker, which knows the prelude's types and functions and the builtins. The root module's names are bare, so a name that only the root declares would otherwise reach the root's item from a module that never imported it. Outside the root file that is reported as not in scope.

### Paths

A path's first segment is looked up like a bare name. A type in scope starts a type path (`Point::new`, or `Pt::new` after an `as` rename), and a module or block in scope starts a module path. A first segment that nothing in scope has starts at the program's root directory, so `io::read` written in `utils/mod.nr` means the root's `io.nr`. Paths in an `import` may instead start with `./`, at the importing file's directory, or with `../`, one directory above it per occurrence.

The rest of the path descends through child modules: a directory module's files and any module's inline blocks. Descent stops at the first segment that names no module. Then one segment left is an item, and two are a type and its member. A two-segment path whose first segment names nothing is a type the checker owns (`Option::Some`, `Vec::new`). Three or more segments can only be a module path, so that is an error.

Paths work in every position a name does: calls, types, patterns (`shapes::Shape::Circle(r)`), struct literals with a base (`geo::Point { x, ..base }`), `impl geo::Drawable for T`, `dyn geo::Drawable` and bounds.

### Discovery

Discovery follows references. A path loads the modules it descends through, and an `import` loads the modules it names. Nothing globs a directory, so unrelated programs beside a file never join its build. Discovery reads a path's first segment exactly as the rewrite will. A type or block in scope stops it, an imported name substitutes the import's path, and anything else starts at the root. Each `{...}` entry of an import extends its path, since `import ./utils::{io}` may name a child module rather than an item.

A leaf module has no children. `math.nr` is a file, so `math::io::read` cannot reach an `io.nr` beside it. Only a directory with a `mod.nr` opens a level. A directory named in a path but missing its `mod.nr` is an error, not a silent miss.

### Inline modules

An inline `module Name { ... }` block is a module with no file. The loader lifts it out of its parent and registers it as an ordinary graph module that records its parent, so visibility and canonical names apply unchanged. Blocks nest. A block holds no file children, so a path through one stops where it stops for `math.nr`. A block in scope wins over a same-named file at the root. Two blocks in one module may not share a name, and a root-file block may not share its path with a root-level file that is also loaded.

A block sees every name of the modules it is written in, sibling blocks included, and may use their private items, because it is part of them. The reverse does not hold: the module that declares a block needs the block's `export`s like anyone else. A block written without `export module` is reachable only from inside the module that declares it.

### Visibility

A declaration is private to its module unless it is written with `export`. This pass decides it, because it is the last one that knows both the declaring module and the referencing one. A qualified path and an import both go through one visibility check, which passes a module, or a block inside it, naming its own declarations. In `mod::Type::member`, the check applies to `Type`, since a method or variant carries the visibility of its type.

Field visibility is the exception. Checking `c.timeout` needs the receiver's type, and that is known only after this pass. So each item carries the module it came from (`ast_types::ModuleId`), and the type checker compares that with the module of the access.

### Imports

Each module's imports become one import scope: module aliases, items and variant bindings. A module may not bind one name twice, and an import may not bind a name the module declares itself. `as` renames one of them. A local of the same name shadows any import.

`import Shape::{Circle}` imports a list of variants and `import Shape::Circle` a single one. Either is read as a variant import only when no module is named `Shape` and an enum `Shape` is in scope: declared by the module or one around it, imported by an earlier line or by a module around it, or the prelude's. Otherwise it is `UnknownImportHead`.

### Re-exports

`export import` lets a module offer a name that it does not declare. The importing module records where the declaration really lives, so `facade::Config` resolves straight to `internal::Config`, and any `as` rename is undone along the way. Only an item can be re-exported. A module or an enum variant is reached through something else, so `export import` on one is an error. Re-exports settle to a fixpoint before import scopes are built, since a chain resolves one link per round.

### Trait defaults

The parser copies a trait's default methods into the impls of the same file. A trait and an impl in different modules, or inside blocks, meet only once names are canonical, so this pass copies each default body into every `impl Trait for Type` that does not write the method. The body was resolved in the trait's module, so the copy names what that module meant.

### Prelude

The prelude's variants (`Some`, `None`, `Ok`, `Err`) are looked up after every scope from the module outward. The prelude is therefore the weakest binding: an explicit import or a local declaration of `Some` keeps its meaning.

`@no_prelude` on a file gives that file none of the variant bindings. The prelude's declarations join the program as one list, so the root file's `@no_prelude` decides whether they are prepended at all. It is reported back as `ResolvedProgram.no_prelude`.

### Spans

This slice never touches a span. The driver adds each file to the program's source map before parsing it and parses it at the base the map returns, so every file, the prelude included, has its own range in one offset space. A span from any module therefore names its own file, and a compile-time error or a runtime panic reports that file's line and column.

## Diagnostics

`neurc check` prints a `ModuleError` as `Error: Module error: ...` and exits 1. Each message names the file that holds the bad reference. Messages a user hits most often:

| Situation | Message |
|---|---|
| Reaching an item with no `export` | ``secret` is private to module `math` and cannot be used from `main.nr`; write `export` before its declaration to make it module-public`` |
| A root-file name used in another module | ``secret` is declared in `main.nr`, not in `lib.nr`, and nothing in `lib.nr` imports it; every module is a namespace, so import it or write its path`` |
| Two imports of one name | ``size` is imported twice in `main.nr`; one name can stand for one thing, so rename one of them with `as`` |
| An import of a name the module declares | ``size` is imported in `main.nr`, which declares a `size` of its own; one name can stand for one thing, so rename the import with `as`` |
| A directory in a path with no `mod.nr` | ``dir` is a directory with no `mod.nr`, so it is not a module (referenced from `main.nr`); add `dir/mod.nr` or point the path at a `.nr` file`` |
| A path of three segments naming no module | ``nope::a::b` does not name a module (referenced from `main.nr`); expected `nope.nr` or `nope/mod.nr` beside it`` |
| An import of a name the module lacks | ``module `math` declares no item named `nope` (referenced from `main.nr`)`` |
| An import head that is neither a module nor an enum | ``Shape` in `main.nr` names neither a module nor an enum, so the import brings nothing into scope`` |
| `export import` of a module or variant | ``export import` in `main.nr` would re-export `x`, which names a module rather than an item; only an item can be re-exported; drop the `export``` |
| A bare path to a block from another file | ``leaf::Thing` in `other.nr` cannot reach the inline module `leaf`: a block is in scope inside the module that declares it, and from anywhere else is reached through that module's path`` |

## Testing

- [`compiler/module-resolution/src/tests.rs`](../../../compiler/module-resolution/src/tests.rs) holds the unit tests. They write small module trees to a temporary directory and use a stub parser.
- The end-to-end module programs are in `compiler/neurc/tests/suite/`, in `modules.rs`, `module_namespaces.rs`, `imports.rs`, `inline_modules.rs`, `visibility.rs` and `implicit_prelude.rs`.

## Source

- [`compiler/module-resolution/src/lib.rs`](../../../compiler/module-resolution/src/lib.rs)
- [`compiler/module-resolution/CONTEXT.md`](../../../compiler/module-resolution/CONTEXT.md): the reasoning behind discovery, canonical names, name lookup, re-export settling and the prelude

## See Also

- [Modules language reference](../../language-reference/modules.md)
- [Syntax Parsing](syntax-parsing.md)
- [Semantic Analysis](semantic-analysis.md)
