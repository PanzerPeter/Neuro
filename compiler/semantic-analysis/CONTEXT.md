# semantic-analysis

## Purpose
Validate type correctness and scope rules of a parsed NEURO program before code generation is attempted.

## Entry Point
- Type: Library function
- Input: `items: &[Item]`
- Output: `Result<(), Vec<TypeError>>`

## Data Ownership
- Tables: none
- Events Published: none
- Events Consumed: none
- Public Read Model: none

## Shared Kernel
- ast-types — read-only traversal of `Item`, `Expr`, `Stmt` nodes
- shared-types — `Span` embedded in every `TypeError` for diagnostic location
- diagnostics — error type infrastructure

## Notes
Fail-slow strategy: all type errors are collected in a single pass so the developer
sees the complete error set in one compilation. `syntax-parsing` appears only in
`[dev-dependencies]` (integration tests); it is not a production dependency.

Three-pass strategy within `check_program`:
1. Pre-register all `Item::Struct` definitions into `struct_defs`.
2. Pre-register all `Item::Impl` method signatures into `functions` (mangled as
   `StructName__methodName`) and into `impl_methods` (struct → method → mangled key).
3. Full-check pass: `check_function` for each `Item::Function`, `check_impl` for each
   `Item::Impl`.

Struct types use nominal typing — two `Type::Struct` values are compatible iff their
names are equal.

`impl` method scoping: `check_impl` binds `self` as an immutable variable of the
struct type, then binds the remaining parameters, before checking the method body.

Method calls (`instance.method(args)`) are recognised in `check_expr` when the `Call`
node's `func` is a `FieldAccess`. The object's struct type drives a lookup into
`impl_methods` to find the mangled function name, then arity and argument types are
validated against the registered signature (skipping param[0] which is `self`).

Associated function calls (`TypeName::func(args)`) are recognised in `check_expr`
when the `Call` node's `func` is an `Expr::Path`. The mangled name is reconstructed
as `TypeName__funcName` and looked up directly in `functions`.

`&mut self` and consuming `self` methods are rejected at registration time with
`UnsupportedSelfParam` until ownership semantics land (Phase 1.5).
