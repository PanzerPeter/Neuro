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

`Item::Impl` carries an `ImplDef` (type name + list of `MethodDef`). Each `MethodDef` holds an
`Option<SelfParam>` distinguishing associated functions (`None`) from instance methods (`Some`).
`SelfParam::Ref` (`&self`) is the only variant currently supported end-to-end; `RefMut` and
`Owned` are parsed but rejected by semantic analysis until ownership semantics land.

`Expr::Path { type_name, member, span }` represents `TypeName::member` path expressions used as
the callee of associated-function calls (`Point::new(args)`).

## Recent Updates
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
- 2026-06-04: Added `Expr::Unsafe { stmts, span }` for `unsafe { }` block expressions (Phase 1.7 groundwork). Structurally identical to `Expr::Block`; the distinct node lets later phases (Phase 5 `@kernel`) attach the kernel-aliasing relaxation. Inert today — no special semantics.
- 2026-05-20: Added `Attribute { name, args, span }` struct. `FunctionDef` and `MethodDef` now carry `attributes: Vec<Attribute>`. Semantics are interpreted by later passes (e.g. `@allow(prefer_loop_over_while_true)`); unknown attribute names are accepted so the surface stays forward-compatible with future `@grad`, `@gpu`, `@no_prelude`.
- 2026-04-04: Added `inclusive: bool` to `Stmt::ForRange` to support `..=` inclusive range iteration.
- 2026-04-16: Added `ConstDef` struct and `Item::Const(ConstDef)` for module-level constants (§1.3).
  Added `Stmt::Const { name, ty, value, span }` for function-body constants.
- 2026-05-18: Added `BinaryOp::NullCoalesce` (`??`) variant. Carries no semantics here — semantic-analysis rejects it until Phase 2 lands Option/Result. Defined now so the AST shape is final for the parser's R-to-L associativity test.
- 2026-04-28: Added `Expr::If { condition, then_block, else_if_blocks, else_block, span }` and
  `Expr::Block { stmts, span }` for value-producing if-expressions and block expressions (§1.8).
  `expressions.rs` now `use super::statements::Stmt` for the block payload types.
