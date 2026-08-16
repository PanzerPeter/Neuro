# module-resolution

## Purpose
Expand a root `.nr` file into the single item list its program is built from, loading every module a qualified path reaches into and erasing the qualifier.

## Entry Point
- Type: Library function
- Input: `root: &Path`, plus a `&dyn Fn(&str) -> Result<Vec<Item>, String>` parser supplied by the caller
- Output: `Result<ResolvedProgram, ModuleError>` — `items: Vec<ast_types::Item>` and one `ResolvedModule` per loaded file

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
- **Discovery is reference-driven.** There is no `import` yet (that is the next roadmap
  item), so a module is loaded when a qualified path names it: `math::sqrt` looks for
  `math.nr` or `math/mod.nr` beside the referencing file. Globbing a directory instead
  would drag every unrelated single-file program in `examples/` into one build.
- **A leaf module has no children.** `math.nr` is a file, so `math::io::read` cannot reach
  `io.nr` beside `math.nr`; only a directory with a `mod.nr` opens a level. A directory
  named in a path but *missing* its `mod.nr` is an error rather than a silent miss —
  that is the one case where "no file" is certainly a mistake.
- **A locally declared type wins over a same-named file.** `Point::new` keeps meaning the
  associated function even when a `Point.nr` sits next door, so adding a file can never
  silently re-point an existing path.
- **Descent stops, it does not fail.** A segment naming no module ends the descent and the
  remainder is left for the type checker: that is how `Point::new` and `Option::Some`
  survive this pass untouched. Only a path of three or more segments whose head resolves
  to nothing is an error — it can have been meant as nothing but a module path.
- **The merge is flat, and collisions are reported.** Every module's items join one
  namespace; qualifiers are verified against the owning module and then stripped, so
  semantic analysis, HIR lowering, and both backends never learn that modules exist. The
  cost is that two modules cannot each declare `helper` — that is a hard error naming both
  files, not a silent winner. Per-module private namespaces need the `export` visibility
  rules and land with them.
- **The qualifier rides inside `Identifier::name`.** The parser folds `a::b::c` into a
  qualifier `a::b` plus a member `c` rather than growing an AST node, because resolution
  always runs before semantic analysis and no `::` name survives it. A cross-module struct
  literal `geometry::Point { x: 1.0 }` parses as an enum struct-variant construction (the
  brace form is ambiguous until the qualifier is known) and is rewritten into a plain
  struct literal here.
- **One walk, two passes.** `walk.rs` visits every place a qualified name can be written;
  discovery and rewriting are the same traversal with different callbacks, so a new
  qualified position cannot be handled by one and forgotten by the other.
- Spans stay per-file. Merged modules therefore share one offset space, exactly as the
  driver's prepended prelude already does, so a panic diagnostic from a non-root module
  reports a position in the root file's coordinates. Per-file diagnostic attribution needs
  a multi-source backend contract and is not part of this item.
