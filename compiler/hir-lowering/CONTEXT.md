# hir-lowering

## Purpose
Lower a type-checked surface AST into the typed High-Level IR (`neuro-hir`), re-deriving every expression's resolved type so each backend consumes HIR instead of the AST.

## Entry Point
- Type: Library function
- Input: `items: &[ast_types::Item]` (a program that already passed `semantic_analysis::type_check`)
- Output: `Result<neuro_hir::HirProgram, LoweringError>`

## Data Ownership
- Tables / Events Published / Events Consumed / Public Read Model: none

## Shared Kernel
- ast-types — read-only traversal of the surface `Item` / `Stmt` / `Expr` / `Type` nodes
- neuro-hir — the typed HIR node set produced as output
- shared-types — `Span`, `Literal`, `IntSuffix`, `FloatSuffix`, `Identifier` reused in nodes
- thiserror — `LoweringError` derivation

## Notes
`syntax-parsing` is a `[dev-dependencies]` entry only (tests build ASTs through the parser); it is never a production cross-slice dependency.

Lowering **re-derives** each expression's type rather than importing the checker's `Type`, which would couple two feature slices (VSA: duplicate over couple). It assumes well-typedness — a shape the checker should have rejected surfaces as a `LoweringError`, never a panic.

The lowerer runs a registration pre-pass mirroring the checker's: struct field tables (+ `@derive(Copy/Clone)` intent), `impl` method signatures under mangled `Struct__method` keys, free-function signatures, and module constants. Bodies then lower under a lexical scope stack and a loop-context stack.

Two type derivations are contextual and faithfully mirror the checker:
- **Literals** take a suffix type, else the expected type when it fits the literal's family, else the default `i32`/`f64`.
- A **function/method body's trailing expression** is an implicit return, typed against the declared return type; nested block/`if`-arm tails are typed with no hint.

Three nodes carry a deliberately-chosen type the source has no first-class form for: a `loop` value-expression takes its `break v` type (or `void`); a method-name callee `FieldAccess` carries the call's result type (there is no method value); a `Range` carries `void` (valid only as a `string.slice` argument — the slice lowering reads its bounds directly). Divergent panic-family calls (`panic`/`assert`/`unreachable`) adopt their context's expected type, or `void` in statement position.
