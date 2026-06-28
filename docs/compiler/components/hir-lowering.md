# HIR Lowering

**Status**: Complete (Phase 1.8)
**Crate**: `compiler/hir-lowering`
**Entry Point**: `pub fn lower_program(items: &[Item]) -> Result<HirProgram, LoweringError>`

## Overview

The HIR lowering slice turns a type-checked surface AST into the typed High-Level IR
(`neuro-hir`), the backend-agnostic contract every backend lowers from. Its defining job is to
attach a fully resolved type to every expression: the frontend type checker validates types but does
not expose them, so the lowerer **re-derives** each expression's type while walking the AST.

`neurc` runs lowering immediately after `semantic_analysis::type_check` in both `check` and
`compile`. The output feeds the [LLVM backend](llvm-backend.md) (and the
[MLIR backend](mlir-backend.md) under the `mlir` feature).

## Architecture

- **Dependencies**: `ast-types` (read-only AST traversal), `neuro-hir` (output node set),
  `shared-types`, `thiserror` (`LoweringError`). `syntax-parsing` is a **dev-dependency only**
  (tests build ASTs through the parser) — never a production cross-slice dependency.
- **Public API**: single `lower_program` entry point + `LoweringError`.
- **No semantic coupling**: lowering re-derives types rather than importing
  `semantic_analysis::Type` — importing it would couple two feature slices, which VSA forbids
  (duplicate over couple).

## Behavior

Lowering **assumes well-typedness** — it computes types, it does not validate them. A shape the
checker should have rejected surfaces as a `LoweringError`, never a panic.

A registration pre-pass mirrors the checker's: struct field tables (plus `@derive(Copy/Clone)`
intent), `impl` method signatures under mangled `Struct__method` keys, free-function signatures, and
module constants. Bodies then lower under a lexical scope stack and a loop-context stack.

Two type derivations are contextual, faithfully mirroring the checker:

- **Literals** take a suffix type, else the expected type when it fits the literal's family, else the
  default `i32` / `f64`.
- A **function/method body's trailing expression** is an implicit return, typed against the declared
  return type; nested block / `if`-arm tails are typed with no hint.

Three nodes carry a deliberately-chosen type the source has no first-class form for:

- a `loop` value-expression takes its `break v` type (or `void`);
- a method-name callee (`FieldAccess`) carries the call's result type (there is no method value);
- a `Range` carries `void` (valid only as a `string.slice` argument — slice lowering reads its bounds
  directly).

Divergent panic-family calls (`panic` / `assert` / `unreachable`) adopt their context's expected
type, or `void` in statement position. The AST's `Expr::Paren` grouping node is dropped — tree
structure already encodes grouping.

## Testing

11 slice unit tests cover the lowering rules; `neurc/tests/hir_lowering.rs` provides end-to-end
coverage. The workspace architecture test enforces the slice's infrastructure-only dependencies.

## Resources

- [neuro-hir CONTEXT](../../../compiler/infrastructure/neuro-hir/CONTEXT.md) — the HIR node set
- [hir-lowering CONTEXT](../../../compiler/hir-lowering/CONTEXT.md) — slice contract
