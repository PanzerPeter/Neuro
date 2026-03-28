# shared-types

## Purpose
Provide lightweight, zero-business-logic data structures used universally across all compiler slices: source spans, identifiers, and literal values.

## Entry Point
- Type: Library (no entry function — pure data)
- Public types: `Span`, `Identifier`, `Literal`

## Data Ownership
- Tables: none
- Events Published: none
- Events Consumed: none
- Public Read Model: none

## Shared Kernel
No upstream dependencies within the NEURO workspace. This is the lowest-level infrastructure crate.

## Notes
`Span` is a half-open byte-offset range `[start, end)` used by every AST node and token for accurate error reporting. `Identifier` wraps a `String` name with a `Span`. `Literal` enumerates all compile-time constant value kinds (integer, float, string, bool).
