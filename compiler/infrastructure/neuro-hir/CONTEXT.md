# neuro-hir

## Purpose
Provide the typed High-Level IR (HIR) node definitions — the stable, backend-agnostic contract between the frontend (parser + type checker) and all backends (`llvm-backend`, `mlir-backend`).

## Entry Point
- Type: Library (no entry function — pure data)
- Public types: `HirProgram`, `HirItem`, `HirFunction`, `HirParam`, `HirStruct`, `HirField`,
  `HirImpl`, `HirMethod`, `HirSelfParam`, `HirConst`, `HirStmt`, `HirExpr`, `HirExprKind`,
  `HirFieldInit`, `HirType`

## Data Ownership
- Tables / Events Published / Events Consumed / Public Read Model: none

## Shared Kernel
- shared-types — `Span`, `Literal` embedded in HIR nodes
- ast-types — `BinaryOp`, `UnaryOp` operator enums reused unchanged (pure data enums,
  identical between surface and IR; reused to avoid a pointless duplicate + conversion)

## Notes
The HIR mirrors the surface AST (`ast-types`) one-to-one in structure, with two defining
differences that make it the *typed* contract:

1. **Every expression carries its resolved type.** `HirExpr` is `{ kind, ty, span }`; `ty` is a
   fully resolved `HirType`. `HirType` has **no `Unknown` variant** — reaching the HIR implies the
   program type-checked. Its variant set mirrors the resolved types the semantic analyzer produces
   today (§1.2); no tensor/generic variants until the language gains them (No Speculative Generality).
2. **Syntactic noise is normalized away.** The AST's `Expr::Paren` grouping node is dropped — tree
   structure already encodes grouping. Identifiers are resolved to their `String` name; the source
   span lives on the enclosing node.

Scope of Phase 1.8 item 2: this crate defines the HIR types only. The AST → HIR lowering (item 3)
and the `llvm-backend` migration onto HIR (item 4) are separate pipeline steps that produce/consume
these types; they are intentionally not part of this crate. No frontend-only data (attributes such
as `@allow` lint suppression) is carried into the HIR — those are consumed before lowering. Backend
attributes (`@grad`, `@gpu`) will be added here when the features that need them land.
