# ast-types

## Purpose
Provide the canonical Abstract Syntax Tree (AST) node definitions shared by all compiler stages that produce or consume the AST — without coupling them to each other.

## Entry Point
- Type: Library (no entry function — pure data)
- Public types: `Item`, `Expr`, `Stmt`, `BinaryOp`, `UnaryOp`, `TypeAnnotation`, `FunctionParam`

## Data Ownership
- Tables: none
- Events Published: none
- Events Consumed: none
- Public Read Model: none

## Shared Kernel
- shared-types — `Span`, `Identifier`, `Literal` embedded in every AST node

## Notes
Extracted from `syntax-parsing` to eliminate the cross-slice dependency that `semantic-analysis` and `llvm-backend` previously had on `syntax-parsing`. All three consumer slices now depend only on this infrastructure crate, not on each other. `syntax-parsing/src/ast/mod.rs` re-exports all types from here for backwards compatibility.
