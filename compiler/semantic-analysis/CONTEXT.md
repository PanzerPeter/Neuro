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

Struct checking uses a two-pass strategy within `check_program`: the first pass
registers all `Item::Struct` definitions into `struct_defs` so that struct type names
resolve correctly when encountered inside function signatures and bodies. The second
pass runs `check_function` for each function. Struct types use nominal typing —
two `Type::Struct` values are compatible iff their names are equal.
