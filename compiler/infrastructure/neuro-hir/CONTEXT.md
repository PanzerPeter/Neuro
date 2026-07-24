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
   today; no tensor/generic variants until the language gains them (No Speculative Generality).
2. **Syntactic noise is normalized away.** The AST's `Expr::Paren` grouping node is dropped — tree
   structure already encodes grouping. Identifiers are resolved to their `String` name; the source
   span lives on the enclosing node.

## Recent Updates
- 2026-07-24: Closures and lambdas. Added `HirItem::Closure(HirClosure { name, captures, params, return_type, body, span })` — one lifted item per closure literal, whose first (implicit) parameter at codegen is the captured-environment pointer — and `HirExprKind::Closure { name, captures }`, the closure value that references its lifted item and lists the enclosing variables to snapshot (in capture-layout order). Added `HirCapture { name, ty }`. The value's `ty` is the existing `HirType::Function { params, ret }` (previously only used for function references). Re-exported `HirClosure` and `HirCapture` from the crate root.
- 2026-07-19: Static & dynamic dispatch. Added `HirType::DynObject(String)` (a trait object, valid only as a `HirType::Reference` referent; backends lower `&dyn T` to a `{ data ptr, vtable ptr }` fat pointer), `HirExprKind::DynCoerce { value }` (the `&T` -> `&dyn Trait` unsizing coercion — `value.ty` names the concrete type that selects the vtable, the node's `ty` is the trait-object reference), and `HirItem::Trait(HirTrait { name, methods, span })`. `HirTrait` exists ONLY to give dynamic dispatch a canonical vtable slot order (the trait's declaration order); static-dispatch traits remain fully erased. Re-exported `HirTrait` from the crate root.
- 2026-07-02: Newtype declarations. Added `HirType::Newtype { name, inner }` (a nominal wrapper
  that carries its resolved inner type so backends can erase it) and the transparent expression kinds
  `HirExprKind::NewtypeConstruct { name, value }` (`Name(value)`) and `HirExprKind::NewtypeAccess {
  object }` (`.0`). A newtype produces no `HirItem` — it dissolves into these types/nodes, which both
  backends lower straight through to the inner value.
- 2026-07-02: Pattern matching. Added `HirExprKind::Match { scrutinee, arms }` with the fully
  resolved `HirMatchArm { tests, bindings, guard, body }`. `HirMatchTest::{Wildcard, Tag, IntEq,
  IntRange}` are the refutable tests (an exclusive `a..b` is pre-normalized to `a..=b-1`);
  `HirMatchBinding { name, ty, source }` with `HirBindingSource::{Scrutinee, EnumPayload { slot }}`
  describes each binding so the backend needs no pattern/exhaustiveness logic.
- 2026-06-30: Enums with associated data. Added `HirType::Enum(String)` (nominal), `HirItem::Enum`
  with `HirEnum { name, variants }` / `HirEnumVariant { name, fields }` / `HirEnumField { name:
  Option<String>, ty }`, and `HirExprKind::EnumConstruct { enum_name, variant, tag, payload }` — the
  single node all three surface construction forms normalize to (payload in declared field order; `tag`
  is the variant's declaration index). Consumed by both backends.
- 2026-06-29: Struct + array destructuring. Added `HirExprKind::ArrayRest { array, start }`, the
  typed mirror of the AST's array-rest node; its `ty` carries the resolved `[T; N - start]` remainder
  type. Struct destructuring carries no HIR node (the parser desugars it to field accesses).
- 2026-06-28: Tuples. Added `HirType::Tuple(Vec<HirType>)` (with `(T1, T2, ...)` Display) and the
  `HirExprKind::TupleLiteral { elements }` / `HirExprKind::TupleIndex { object, index }` expression
  kinds — the typed mirror of the AST's tuple nodes. Destructuring carries no HIR node (the parser
  desugars it before lowering).

Scope of 1D item 2: this crate defines the HIR types only. The AST → HIR lowering (item 3)
and the `llvm-backend` migration onto HIR (item 4) are separate pipeline steps that produce/consume
these types; they are intentionally not part of this crate. No frontend-only data (attributes such
as `@allow` lint suppression) is carried into the HIR — those are consumed before lowering. Backend
attributes (`@grad`, `@gpu`) will be added here when the features that need them land.
