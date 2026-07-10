# ast-types

## Purpose
Provide the canonical Abstract Syntax Tree (AST) node definitions shared by all compiler stages that produce or consume the AST — without coupling them to each other.

## Entry Point
- Type: Library (no entry function — pure data)
- Public types: `Item`, `Expr`, `Stmt`, `BinaryOp`, `UnaryOp`, `TypeAnnotation`, `FunctionParam`,
  `ImplDef`, `MethodDef`, `SelfParam`, `Attribute`

## Data Ownership
- Tables / Events Published / Events Consumed / Public Read Model: none

## Shared Kernel
- shared-types — `Span`, `Identifier`, `Literal` embedded in every AST node

## Notes
Extracted from `syntax-parsing` to eliminate the cross-slice dependency that `semantic-analysis` and `llvm-backend` previously had on `syntax-parsing`. All three consumer slices now depend only on this infrastructure crate, not on each other. `syntax-parsing/src/ast/mod.rs` re-exports all types from here for backwards compatibility.

`Item::Impl` carries an `ImplDef` (optional `trait_name` + type name + list of `MethodDef`).
`trait_name` is `Some` for a trait implementation (`impl Drop for T`, §2.1) and `None` for an
inherent block (`impl T`). Each `MethodDef` holds an
`Option<SelfParam>` distinguishing associated functions (`None`) from instance methods (`Some`).
`SelfParam::Ref` (`&self`) is the only variant currently supported end-to-end; `RefMut` and
`Owned` are parsed but rejected by semantic analysis until ownership semantics land.

`Expr::Path { type_name, member, span }` represents `TypeName::member` path expressions used as
the callee of associated-function calls (`Point::new(args)`).

## Recent Updates
- 2026-07-06: Generic structs & impls §3.8. Added `StructDef.generics: Vec<GenericParam>`
  (empty for a non-generic struct), `ImplDef.generics: Vec<GenericParam>` + `ImplDef.type_args:
  Vec<Type>` (the `<T>` of `impl<T> Wrapper<T>`), and a new `Type::Generic { name, args, span }`
  variant for a generic type application `Name<T1, ...>`. A bare type-parameter reference stays a
  plain `Type::Named`. Interpreted by semantic-analysis and hir-lowering (monomorphization).
- 2026-07-03: Generic functions §3.8. Added `GenericParam { name, bounds, span }` and
  `FunctionDef.generics: Vec<GenericParam>` (empty for a non-generic function). A generic
  function is a template; a type-parameter reference in an annotation is a plain `Type::Named`
  (resolved against the generics in scope by later passes). `bounds` records trait names for
  forward compatibility but is not enforced (no trait system yet). Interpreted by
  semantic-analysis (inference/monomorphization checking) and hir-lowering (monomorphization).
- 2026-07-02: Newtype declarations §3.15. Added `NewtypeDef { name, inner, span }` and
  `Item::Newtype(NewtypeDef)`. Unlike a `type` alias (expanded at parse time), a newtype survives as
  its own item; a newtype *type* annotation is a plain `Type::Named`, construction reuses
  `Expr::Call(Identifier)`, and inner access reuses `Expr::TupleIndex` (`.0`). Interpreted by
  semantic-analysis, hir-lowering, and both backends.
- 2026-07-02: Pattern matching §3.6. Added `Expr::Match { scrutinee, arms, span }` (with its `span()`
  arm) and the pattern types `MatchArm { patterns, guard, body, span }`, `Pattern::{Wildcard, Binding,
  Literal, Range, Enum}` (with `Pattern::span()`), `EnumPatternPayload::{Unit, Tuple, Struct}`, and
  `FieldPattern { field, pattern, span }`. Payload sub-patterns are restricted to bindings/`_` in this
  phase; or-patterns cannot bind (enforced in semantic analysis).
- 2026-06-30: Enums with associated data §3.5. Added `Item::Enum(EnumDef)`; `EnumDef { name, variants,
  span }`, `EnumVariant { name, payload, span }`, and `VariantPayload::{Unit, Tuple(Vec<Type>),
  Struct(Vec<FieldDef>)}`. Added `Expr::EnumStructLiteral { enum_name, variant, fields, span }` for the
  brace construction form (`E::V { f: x }`); unit/tuple variants reuse `Expr::Path` / `Expr::Call(Path)`
  and are disambiguated against the enum table in later passes. An enum *type* annotation is a plain
  `Type::Named`. Interpreted by semantic-analysis, hir-lowering, and llvm-backend.
- 2026-06-29: Struct + array destructuring §3.2. Added `Expr::ArrayRest { array, start, exact, span }`,
  a compiler-internal node the array-pattern desugar produces for the trailing `..rest` sub-slice
  (`[T; N - start]`), with its `span()` arm. `exact` records a rest-less pattern (length must match
  exactly). Struct destructuring `val Point { x, y } = p` needs no AST node (desugars to field-access
  bindings); only the array remainder does, because its size is known only after type checking.
- 2026-06-28: Tuples §3.2. Added `Type::Tuple { elements, span }` (the `(T1, T2, ...)` type),
  `Expr::TupleLiteral { elements, span }` (the `(e0, e1, ...)` literal, always ≥2 elements), and
  `Expr::TupleIndex { object, index, span }` (the `t.0` / `t.1` constant-index access, distinct from
  `FieldAccess` which names a struct field), with their `span()` arms. Destructuring `val (a, b) = e`
  needs no AST node — the parser desugars it to a temp binding plus indexed bindings.
- 2026-06-19: Arrays §3.1. Added `Type::Array { element, size, span }`, `Expr::ArrayLiteral { elements, span }`,
  `Expr::Index { object, index, span }`, `Stmt::ForEach { label, iterator, iterable, body, span }`, and
  `Stmt::IndexAssignment { target, index, value, span }`, with their `span()` arms.
- 2026-06-18: String `.slice(range)` (§2.7). Added `Expr::Range { start, end, inclusive, span }`,
  the `a..b` / `a..=b` node. Not a first-class value: it is only valid as a `string.slice`
  argument (semantic-analysis rejects it elsewhere). `for`-range loops keep their bounds on
  `Stmt::ForRange` and never produce this node.
- 2026-06-15: `loop` as a value expression (§3.7). Added `Expr::Loop { label, body, span }` — the
  value-producing form, distinct from `Stmt::Loop` (statement form, value discarded). `Stmt::Break`
  gained `value: Option<Expr>` for `break v`. The targeted `loop` evaluates to its value-`break`s
  (which must agree on type); `while`/`for` stay unit and have no expression form.
- 2026-06-15: Loop labels (§3.7). `Stmt::While` / `ForRange` / `Loop` each gained `label:
  Option<Identifier>` (the `outer:` prefix); `Stmt::Break` / `Continue` each gained `label:
  Option<Identifier>` (`break outer`). `None` is the unlabeled form. Resolved by semantic-analysis
  (a label stack) and llvm-backend (labeled `LoopTargets`).
- 2026-06-09: Added `Stmt::Loop { body, span }` for the infinite `loop { ... }` statement (§3.7).
  Distinct from `While`: no condition, the only exit is `break`, `continue` re-enters from the top.
  The value-producing `break value` form is not modelled yet — a `loop` statement yields unit.
  Interpreted by semantic-analysis (`loop_depth` so `break`/`continue` are in-loop) and llvm-backend
  (unconditional back-edge).
- 2026-06-09: Mutable borrows `&mut T` (§2.5). `Type::Reference` and `Expr::Reference` gained a
  `mutable: bool` field (`&mut T` / `&mut place`). New `Expr::Deref { operand, span }` (the prefix
  `*` dereference) and `Stmt::DerefAssignment { pointer, value, span }` (`*r = value`). Interpreted
  by semantic-analysis (`&mut` needs a `mut` binding; `*` reads/writes; `&mut T` ≠ `&T`) and
  llvm-backend (borrow → storage pointer; deref → load/store).
- 2026-06-08: Added `Type::Reference { inner, span }` (`&T`) and `Expr::Reference { operand, span }`
  (`&place`) for immutable borrows (§2.4). The reference type appears in any type-annotation
  position; the borrow expression is a prefix `&` on a place expression. Interpreted by
  semantic-analysis (no move, `Copy`, auto-deref) and llvm-backend (lowered to an opaque pointer).
- 2026-06-07: `StructDef` gained `attributes: Vec<Attribute>` so `@derive(Copy, Clone)` (§2.3) can
  attach to struct definitions. Mirrors the existing `attributes` field on `FunctionDef` /
  `MethodDef`; interpreted by semantic-analysis. Empty when no attributes are present.
- 2026-06-05: `Expr::StructLiteral` gained `base: Option<Box<Expr>>` for functional-update syntax
  (`Point { x: 1.0, ..p }`, §3.3). `None` is a plain literal (all fields listed). Field-init
  shorthand (`Point { x, y }`) needs no AST change — the parser desugars a bare field to
  `FieldInit { value: Expr::Identifier(field_name) }`.
- 2026-06-04: Added `Expr::Unsafe { stmts, span }` for `unsafe { }` block expressions (1C groundwork). Structurally identical to `Expr::Block`; the distinct node lets later phases (Phase 4 `@kernel`) attach the kernel-aliasing relaxation. Inert today — no special semantics.
- 2026-05-20: Added `Attribute { name, args, span }` struct. `FunctionDef` and `MethodDef` now carry `attributes: Vec<Attribute>`. Semantics are interpreted by later passes (e.g. `@allow(prefer_loop_over_while_true)`); unknown attribute names are accepted so the surface stays forward-compatible with future `@grad`, `@gpu`, `@no_prelude`.
- 2026-04-04: Added `inclusive: bool` to `Stmt::ForRange` to support `..=` inclusive range iteration.
- 2026-04-16: Added `ConstDef` struct and `Item::Const(ConstDef)` for module-level constants (§1.3).
  Added `Stmt::Const { name, ty, value, span }` for function-body constants.
- 2026-05-18: Added `BinaryOp::NullCoalesce` (`??`) variant. Carries no semantics here — semantic-analysis rejects it until 1G lands Option/Result. Defined now so the AST shape is final for the parser's R-to-L associativity test.
- 2026-04-28: Added `Expr::If { condition, then_block, else_if_blocks, else_block, span }` and
  `Expr::Block { stmts, span }` for value-producing if-expressions and block expressions (§1.8).
  `expressions.rs` now `use super::statements::Stmt` for the block payload types.
- 2026-07-10: Const generics / `where` / turbofish (§3.8). New public types `ArraySize`
  (`Literal`/`Const`), `GenericArg` (`Type`/`Const`), `GenericParamKind` (`Type`/`Const`). `GenericParam`
  gains `kind`; `Type::Array.size` is `ArraySize`; `Type::Generic.args` is `Vec<GenericArg>`;
  `Expr::Call` gains `type_args`; `FunctionDef`/`StructDef`/`ImplDef` gain `where_predicates`.
