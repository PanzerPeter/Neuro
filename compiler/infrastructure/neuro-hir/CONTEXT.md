# neuro-hir

## Purpose
Provide the typed High-Level IR (HIR) node definitions — the stable, backend-agnostic contract between the frontend (parser + type checker) and all backends (`llvm-backend`, `mlir-backend`).

## Entry Point
- Type: Library (no entry function — pure data)
- Public types: `HirProgram`, `HirItem`, `HirFunction`, `HirParam`, `HirStruct`, `HirField`,
  `HirEnum`, `HirEnumVariant`, `HirEnumField`, `HirImpl`, `HirMethod`, `HirSelfParam`, `HirConst`,
  `HirStmt`, `HirExpr`, `HirExprKind`, `HirFieldInit`, `HirType`

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

## Recent Updates
- 2026-06-30: Enums with associated data §3.5. Added `HirType::Enum(String)` (nominal), `HirItem::Enum`
  with `HirEnum { name, variants }` / `HirEnumVariant { name, fields }` / `HirEnumField { name:
  Option<String>, ty }`, and `HirExprKind::EnumConstruct { enum_name, variant, tag, payload }` — the
  single node all three surface construction forms normalize to (payload in declared field order; `tag`
  is the variant's declaration index). Consumed by both backends.
- 2026-06-29: Struct + array destructuring §3.2. Added `HirExprKind::ArrayRest { array, start }`, the
  typed mirror of the AST's array-rest node; its `ty` carries the resolved `[T; N - start]` remainder
  type. Struct destructuring carries no HIR node (the parser desugars it to field accesses).
- 2026-06-28: Tuples §3.2. Added `HirType::Tuple(Vec<HirType>)` (with `(T1, T2, ...)` Display) and the
  `HirExprKind::TupleLiteral { elements }` / `HirExprKind::TupleIndex { object, index }` expression
  kinds — the typed mirror of the AST's tuple nodes. Destructuring carries no HIR node (the parser
  desugars it before lowering).

Scope of 1D item 2: this crate defines the HIR types only. The AST → HIR lowering (item 3)
and the `llvm-backend` migration onto HIR (item 4) are separate pipeline steps that produce/consume
these types; they are intentionally not part of this crate. No frontend-only data (attributes such
as `@allow` lint suppression) is carried into the HIR — those are consumed before lowering. Backend
attributes (`@grad`, `@gpu`) will be added here when the features that need them land.
