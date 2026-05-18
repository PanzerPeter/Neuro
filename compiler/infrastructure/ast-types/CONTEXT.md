# ast-types

## Purpose
Provide the canonical Abstract Syntax Tree (AST) node definitions shared by all compiler stages that produce or consume the AST — without coupling them to each other.

## Entry Point
- Type: Library (no entry function — pure data)
- Public types: `Item`, `Expr`, `Stmt`, `BinaryOp`, `UnaryOp`, `TypeAnnotation`, `FunctionParam`,
  `ImplDef`, `MethodDef`, `SelfParam`

## Data Ownership
- Tables: none
- Events Published: none
- Events Consumed: none
- Public Read Model: none

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
- 2026-04-04: Added `inclusive: bool` to `Stmt::ForRange` to support `..=` inclusive range iteration.
- 2026-04-16: Added `ConstDef` struct and `Item::Const(ConstDef)` for module-level constants (§1.3).
  Added `Stmt::Const { name, ty, value, span }` for function-body constants.
- 2026-05-18: Added `BinaryOp::NullCoalesce` (`??`) variant. Carries no semantics here — semantic-analysis rejects it until Phase 2 lands Option/Result. Defined now so the AST shape is final for the parser's R-to-L associativity test.
- 2026-04-28: Added `Expr::If { condition, then_block, else_if_blocks, else_block, span }` and
  `Expr::Block { stmts, span }` for value-producing if-expressions and block expressions (§1.8).
  `expressions.rs` now `use super::statements::Stmt` for the block payload types.
