# module-resolution

## Purpose
Expand a root `.nr` file into the single item list its program is built from, loading every module a qualified path reaches into — file, directory, or inline `module { }` block — enforcing what each module exports, binding the implicit prelude's variant names, and erasing the qualifier.

## Entry Point
- Type: Library function
- Input: `root: &Path`, a `&dyn Fn(&str) -> Result<Vec<Item>, String>` parser supplied by the caller, and `prelude: &[PreludeVariant]` — the enum variants every module may write bare
- Output: `Result<ResolvedProgram, ModuleError>` — `items: Vec<ast_types::Item>`, each stamped with the module it came from, one `ResolvedModule` per loaded file, and `no_prelude`, the root file's opt-out

## Data Ownership
- Tables / Events Published / Events Consumed / Public Read Model: none
- Reads `.nr` files from disk; writes nothing

## Shared Kernel
- ast-types — the `Item` / `Stmt` / `Expr` / `Type` tree this slice walks and rewrites
- shared-types — `Identifier`, `Span` on the nodes it rebuilds
- thiserror — `ModuleError` derivation

## Notes
- The parser is **injected, not imported**. This slice would otherwise depend on
  `syntax-parsing`, a feature slice; `neurc` is the one place both meet, and it passes
  `syntax_parsing::parse` in. The slice's own unit tests use a stub parser, so it builds
  and tests with no feature-slice dependency at all.
- **Discovery is reference-driven.** A module is loaded when an `import` names it or a
  qualified path reaches into it: `math::sqrt` looks for `math.nr` or `math/mod.nr` beside
  the referencing file. Globbing a directory instead would drag every unrelated single-file
  program in `examples/` into one build. An import's path is a discovery chain like any
  other, and each `{...}` entry extends it, since a listed name may be a child module
  (`import ./utils::{io}`) rather than an item.
- **A leaf module has no children.** `math.nr` is a file, so `math::io::read` cannot reach
  `io.nr` beside `math.nr`; only a directory with a `mod.nr` opens a level. A directory
  named in a path but *missing* its `mod.nr` is an error rather than a silent miss —
  that is the one case where "no file" is certainly a mistake.
- **An inline `module { }` block is a module with no file.** The loader lifts each block out
  of its parent's items and registers it as an ordinary graph module, so the visibility rule,
  the flat merge, the collision check, and the `ModuleId` stamp all reach it without a special
  case, and blocks nest for free. Consequences that follow rather than being decided
  separately: a block's items are private to the *block*, so the file declaring it is outside
  it and needs `export` like anyone else; a block resolves a path against the containing
  file's directory, so `import ./utils` reads the same written inside a block or beside it;
  and a block holds no file children, so a path through one stops exactly where `math.nr`
  does. A block wins over a same-named file, for the reason a locally declared type does:
  adding a file must never silently re-point a path that already resolved. Two blocks in one
  module may not share a name.
- **`export import` is the one way a name reaches through a module it was not declared in.**
  The importing module records where the declaration really lives, so `facade::Config` lands
  on `internal`'s `Config` in a single step and any `as` rename is undone on the way — the
  flat namespace holds the declaration's own name. Only an *item* can be re-exported: a
  module and an enum variant are each reached through something else, so `export import` on
  one is an error rather than a silent no-op. Re-exports settle to a fixpoint before the
  import scopes are built, because a chain resolves one link per round and modules resolve in
  id order; errors are held back until the tables stop growing, so an import that only becomes
  resolvable in a later round is not reported on an earlier one.
- **A locally declared type wins over a same-named file.** `Point::new` keeps meaning the
  associated function even when a `Point.nr` sits next door, so adding a file can never
  silently re-point an existing path.
- **Descent stops, it does not fail.** A segment naming no module ends the descent and the
  remainder is left for the type checker: that is how `Point::new` and `Option::Some`
  survive this pass untouched. Only a path of three or more segments whose head resolves
  to nothing is an error — it can have been meant as nothing but a module path.
- **A declaration is private to its module unless it is written with `export`.** This pass is
  the last one that knows both which file a name was declared in and which file reaches for it,
  so item visibility is settled here: a qualified path and an import are each checked through
  the one `check_visible`, which passes a module referring to its own name. Only the *item* is
  gated — `mod::Type::member` checks `Type`, because a method and a variant carry the
  visibility of the type they belong to.
- **Field visibility is not settled here.** `c.timeout` needs the receiver's type, and this
  pass runs before type checking. Instead, the merge stamps each item with the module it came
  from (`ast_types::ModuleId` on `FunctionDef` / `StructDef` / `ImplDef` / `ConstDef`) and the
  type checker compares the access site's module against the struct's. That stamp is the one
  thing about modules that survives this slice; HIR lowering and both backends still see none
  of it.
- **The prelude is the weakest binding there is.** Every module's scope is seeded with the
  variants the caller names — `Some`, `None`, `Ok`, `Err` — *after* its own imports, and a name
  the module already bound or declares itself is skipped rather than reported. That is what makes
  shadowing the prelude a non-error: `import Reading::{Some}` and a local `func None` each keep
  their meaning, exactly as the module system's rule says they should. The variant list is an
  input rather than a constant here because the prelude's contents belong to the driver, which
  owns the prelude source — the same reason the parser is injected.
- **`@no_prelude` marks a file, and the root decides for the program.** The loader consumes
  `Item::NoPrelude` into the module's own flag, an inline block inherits the file that holds it,
  and a module with the flag is seeded with nothing. The prelude's *declarations* are a different
  question: they join one flat namespace, so they are either in the program or absent from all of
  it, and the root file's flag — reported back as `ResolvedProgram.no_prelude` — is what the
  driver reads to decide. A non-root file's `@no_prelude` therefore takes its bindings, not its
  access to `Option`.
- **An import binds names, it does not gate them.** `imports.rs` turns each file's
  declarations into one `ImportScope` — module aliases, item renames, and variant bindings —
  and the rewriting pass consults the scope of the module it is walking. This is the one
  place the slice is *not* flat: two modules may bind one name to different things, though
  a single module may not bind one name twice. Because the namespace underneath is flat, an
  unrenamed item import is an identity binding: the name already reaches. What the import
  buys is the module load, the validation that the name exists where it was taken from, and
  the renaming and variant forms, none of which the flat namespace gives for free.
- **A variant import is what is left when nothing named a module — and only if an enum by that
  name exists.** `import Option::{Some}` cannot be verified against the loaded modules: `Option`
  comes from the prelude, which the driver prepends *after* this pass — the same reason the
  prelude's own bindings arrive as an argument rather than being read off the program. So a
  single-segment path that names no module is read as an enum and its listed names as variants,
  but `resolve_one` first asks `names_an_enum`: the head must be declared as an `enum` somewhere in
  the graph (the namespace is flat, so anywhere counts) or own a variant in the prelude list.
  Otherwise it is `UnknownImportHead` — the fallback used to swallow a typo, and any path to a
  module that is out of scope, as a binding that quietly meant nothing. A multi-segment head that
  names no module can only have been meant as a module path and is an error.
- **A path that names an invisible block says so.** An inline block sees its own children and its
  file's sibling files, never the file's *other* blocks, so `import leaf::{X}` from a sibling block
  resolves no module. `unresolved_head` checks the graph for an inline block of that name and reports
  `UnreachableInlineModule`, which names the real obstacle instead of guessing at an enum.
- **A bare `None` is a binding until an import says otherwise.** `Some(n)` carries a payload
  and parses as `Pattern::UnqualifiedEnum`, so it is unambiguous and an error when no import
  accounts for it. `None` is written exactly like a catch-all binding, so only the import
  table tells them apart — which is why pattern rewriting lives here rather than in the
  parser.
- **The merge is flat, and collisions are reported.** Every module's items join one
  namespace — an inline block's items included, so a block does not buy a private namespace,
  only a private *surface*; qualifiers are verified against the owning module and then stripped, so
  semantic analysis, HIR lowering, and both backends never learn that modules exist. The cost is
  that two modules cannot each declare `helper`, even privately: `check_name_collisions` runs on
  the merged namespace, independently of `export`, and reports a hard error naming both files
  rather than picking a silent winner. Per-module private namespaces would need a different merge,
  not a visibility rule.
- **The qualifier rides inside `Identifier::name`.** The parser folds `a::b::c` into a
  qualifier `a::b` plus a member `c` rather than growing an AST node, because resolution
  always runs before semantic analysis and no `::` name survives it. A cross-module struct
  literal `geometry::Point { x: 1.0 }` parses as an enum struct-variant construction (the
  brace form is ambiguous until the qualifier is known) and is rewritten into a plain
  struct literal here.
- **One walk, two passes.** `walk.rs` visits every place a qualified or imported name can
  be written — including bare identifiers, `match` / `val-else` patterns (both of which
  imports made significant), and the holes of an interpolated string literal, whose
  expressions are ordinary code and may name imported items; discovery and rewriting are
  the same traversal with different callbacks, so a new position cannot be handled by one
  and forgotten by the other.
- **Rewriting does not track locals.** A bare name is replaced when an import bound it,
  whether or not a local of the same name is in scope. Shadowing an imported name is
  therefore not supported: rename the import with `as`.
- Spans stay per-file. Merged modules therefore share one offset space, exactly as the
  driver's prepended prelude already does, so a panic diagnostic from a non-root module
  reports a position in the root file's coordinates. Per-file diagnostic attribution needs
  a multi-source backend contract and is not part of this item.
