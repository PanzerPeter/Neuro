# module-resolution

## Purpose
Expand a root `.nr` file into the single item list its program is built from: load every module a path reaches (file, directory, or inline `module { }` block), resolve each name against the scope it is written in, enforce what each module exports, and rewrite every reference to the canonical name of the declaration it reaches.

## Entry Point
- Type: Library function, `resolve_program`
- Input: `root: &Path`, a `&ParseModule` (`dyn Fn(&str, &str) -> Result<Vec<Item>, String>`) parser supplied by the caller (called with a module's source and the path that names it in a diagnostic), and `prelude: &[PreludeVariant]`: the enum variants every module may write bare
- Output: `Result<ResolvedProgram, ModuleError>`: `items: Vec<ast_types::Item>`, each stamped with the module it came from and named canonically, one `ResolvedModule` per loaded module (its path and file), and `no_prelude`, the root file's opt-out

## Shared Kernel
- ast-types: the `Item` / `Stmt` / `Expr` / `Type` tree this slice walks and rewrites
- shared-types: `Identifier`, `Span` on the nodes it rebuilds
- thiserror: `ModuleError` derivation

## Notes
Reads `.nr` files from disk; writes nothing.

- The parser is **injected, not imported**. This slice would otherwise depend on
  `syntax-parsing`, a feature slice; `neurc` is the one place both meet, and it passes
  `syntax_parsing::parse` in. The slice's own unit tests use a stub parser, so it builds
  and tests with no feature-slice dependency at all. The parser is handed the module's
  display path along with its source, so the error text it returns is already a rendered
  diagnostic with a location, and `ModuleError::Parse` displays that text as it is.
- **Every module is a namespace, and canonical names carry it downstream.** A declaration in
  a non-root module is renamed `path::Name` (`geometry::Point`, `counter::limits::CEILING`)
  and every reference to it is rewritten to that name; the root's declarations keep their
  bare names, so a single-file program's diagnostics read as they always did. Two modules may
  each declare `helper`: they reach the type checker as `helper` and `stats::helper`. Semantic
  analysis, HIR lowering and both backends therefore still see one item list of distinct names
  and know nothing about modules. The one place a module path is undone is derived `Debug`,
  which prints the declared name (`hir-lowering`'s `written_name`).
- **A module's path is where its file sits under the root directory**, `/` read as `::`, a
  `mod.nr` named by its directory: `utils/io.nr` is `utils::io` whichever module reached it
  first. A file above the root, reached through `../`, keeps a `..` segment per level, so its
  names cannot meet a module's below the root. An inline block's path is its parent's plus its
  own name. Two loaded modules with one path (a root-file block and a root-level file of the
  same name) are `DuplicateModulePath`.
- **Name lookup is lexical.** A bare name written in module M resolves to, in order: a local
  (tracked by the walk), M's imports and its own declarations and inline blocks, the same for
  each module enclosing M when M is a block, then the prelude's variants. A name nothing in
  scope declares is left alone for the type checker, which knows the prelude's types and
  functions and the builtins. Since the root's names are bare, such a name that the root
  happens to declare would land on the root's item from a module that never imported it, so
  outside the root file it is `NotInScope`. Together with canonical names, this keeps a private
  item out of reach by its bare name in either direction.
- **A path's head is looked up the same way; one nothing in scope declares starts at the
  program's root directory.** `io::read` written in `utils/mod.nr` is the root's `io.nr`, not
  `utils/io.nr`; `import ./io` reaches the latter, and `../` climbs above the writing file. A
  type in scope opens a type path (`Point::new`, `Pt::new` after an `as` rename), so a
  same-named file can never re-point it. A head naming nothing at all, in two segments, is a
  type the checker owns (`Option::Some`, `Vec::new`); in three or more it can only have been
  meant as a module path and is `UnknownModule`.
- **Discovery is reference-driven.** A module is loaded when an `import` names it or a path
  reaches into it. Globbing a directory instead would drag every unrelated single-file program
  in `examples/` into one build. Discovery reads a path's head exactly as the rewrite will: a
  type or block in scope stops it, an imported name substitutes the import's path, and anything
  else starts at the root. An import's path is a discovery chain like any other, and each
  `{...}` entry extends it, since a listed name may be a child module (`import ./utils::{io}`).
- **A leaf module has no children.** `math.nr` is a file, so `math::io::read` cannot reach
  `io.nr` beside `math.nr`; only a directory with a `mod.nr` opens a level. A directory
  named in a path but *missing* its `mod.nr` is an error rather than a silent miss.
- **A leading UTF-8 byte order mark is skipped where the file is read**, here and in `neurc`'s
  `read_source`, never in the lexer: stripping it there would shift every span by three bytes.
- **An inline `module { }` block is a module with no file.** The loader lifts each block out
  of its parent's items and registers it as an ordinary graph module with its parent recorded,
  so the visibility rule, the canonical names and the `ModuleId` stamp reach it without a
  special case, and blocks nest for free. A block sees the names of every module it is written
  in (sibling blocks among them) and may use their private items, since it is part of them; the
  reverse does not hold, so the file declaring a block needs the block's `export`s like anyone
  else. A block without `export module` is reached only from inside the module that declares
  it. A block holds no file children, so a path through one stops exactly where `math.nr` does,
  and a block in scope wins over a same-named file.
- **`export import` is the one way a name reaches through a module it was not declared in.**
  The importing module records where the declaration really lives, so `facade::Config` lands
  on `internal::Config` in a single step and any `as` rename is undone on the way. Only an
  *item* can be re-exported: a module and an enum variant are each reached through something
  else, so `export import` on one is an error rather than a silent no-op. Re-exports settle to
  a fixpoint before the import scopes are built, because a chain resolves one link per round
  and modules resolve in id order; errors are held back until the tables stop growing.
- **A declaration is private to its module unless it is written with `export`.** This pass is
  the last one that knows both which module a name was declared in and which one reaches for
  it, so item visibility is settled here through the one `check_visible`, which passes a module
  (or a block inside it) referring to its own name. Only the *item* is gated:
  `mod::Type::member` checks `Type`, because a method and a variant carry the visibility of the
  type they belong to.
- **Field visibility is not settled here.** `c.timeout` needs the receiver's type, and this
  pass runs before type checking. The merge stamps each item with the module it came from
  (`ast_types::ModuleId` on `FunctionDef` / `StructDef` / `ImplDef` / `ConstDef`) and the type
  checker compares the access site's module against the struct's.
- **An import binds names, it does not gate them.** `imports.rs` turns each module's imports
  into one `ImportScope` (module aliases, items, and variant bindings) that the rewrite
  consults. One module may not bind one name twice, and an import may not bind a name the
  module declares itself (`ImportShadowsDeclaration`): each would make one spelling mean two
  things. A local shadows any of them, because the walk never offers a local to the callback.
- **The prelude is the weakest binding there is.** Its variants (`Some`, `None`, `Ok`, `Err`)
  are consulted only after every scope from the module outward, so `import Reading::{Some}` and
  a local `func None` each keep their meaning. The variant list is an input rather than a
  constant here because the prelude's contents belong to the driver, which owns the prelude
  source: the same reason the parser is injected.
- **`@no_prelude` marks a file, and the root decides for the program.** The loader consumes
  `Item::NoPrelude` into the module's own flag, an inline block inherits the file that holds it,
  and a module with the flag sees no prelude variant. The prelude's *declarations* are prepended
  by the driver as one list, so they are in the program or absent from all of it, and the root
  file's flag (reported back as `ResolvedProgram.no_prelude`) decides. A non-root file's
  `@no_prelude` therefore takes its bindings, not its access to `Option`.
- **A variant import is what is left when nothing named a module, and only if an enum by that
  name is in scope**: declared by the module or one enclosing it, imported by an earlier line or
  by an enclosing module (whose finished scope `enum_in_scope` reads, a block being numbered
  after its parent), or one of the prelude's (the driver prepends the prelude after this pass, which is why the
  prelude list is consulted directly). `import Shape::{Circle}` binds a list and
  `import Shape::Circle` one variant. Otherwise the import is `UnknownImportHead`, or
  `UnreachableInlineModule` when the head is a block that is not in scope here.
- **A bare `None` is a binding until a scope says otherwise.** `Some(n)` carries a payload
  and parses as `Pattern::UnqualifiedEnum`, so it is unambiguous and an error when nothing
  in scope accounts for it. `None` is written exactly like a catch-all binding, so only the
  scope tells them apart, which is why pattern rewriting lives here rather than in the parser.
- **A trait's default methods are supplied across modules here.** The parser injects them
  within one file's top level; a trait and an impl in different modules, or inside blocks,
  meet only once names are canonical, so `defaults.rs` copies each default body into every
  `impl Trait for Type` that omits it after the rename. The body was resolved in the trait's
  module, so the copy names what that module meant.
- **The qualifier rides inside `Identifier::name`.** The parser folds `a::b::c` into a
  qualifier `a::b` plus a member `c` rather than growing an AST node, and does the same for a
  qualified trait name, impl header and enum pattern. A cross-module struct literal
  `geometry::Point { x: 1.0 }` parses as an enum struct-variant construction (the brace form is
  ambiguous until the qualifier is known) and is rewritten into a plain struct literal here;
  one with a `..base` is a struct literal from the start.
- **One walk, two passes.** `walk.rs` visits every place a qualified or imported name can be
  written: bare identifiers, `match` / `val-else` patterns, the holes of an interpolated string,
  and every name that can only be a type or a trait (a struct literal's, an `impl` block's type
  and trait, a bound, a supertrait, `impl` / `dyn` types). It also tracks locals (parameters,
  `val` / `var`, pattern and loop bindings, closure parameters, generic parameters) and offers no
  name whose head is one. Discovery and rewriting are the same traversal with different
  callbacks, so a new position cannot be handled by one and forgotten by the other.
- Spans stay per-file, so merged modules share one offset space, exactly as the driver's
  prepended prelude already does: a panic diagnostic from a non-root module reports a position
  in the root file's coordinates, and a compile-time error in a program of more than one module
  prints no location. Per-file attribution needs a multi-source span contract.
