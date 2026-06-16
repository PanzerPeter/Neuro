# shared-types

## Purpose
Provide lightweight, zero-business-logic data structures used universally across all compiler slices: source spans, identifiers, and literal values.

## Entry Point
- Type: Library (no entry function — pure data)
- Public types: `Span`, `Identifier`, `Literal`, `IntSuffix`, `FloatSuffix`

## Data Ownership
- Tables: none
- Events Published: none
- Events Consumed: none
- Public Read Model: none

## Shared Kernel
No upstream dependencies within the Neuro workspace. This is the lowest-level infrastructure crate.

## Notes
`Span` is a half-open byte-offset range `[start, end)` used by every AST node and token for accurate error reporting. `Identifier` wraps a `String` name with a `Span`. `Literal` enumerates all compile-time constant value kinds (integer, float, string, bool, char). `Literal::Char(char)` holds a single Unicode scalar value (§1.2).

`IntSuffix` is a `Copy` enum enumerating the eight integer literal type suffixes (`I8`–`U64`). It is carried by `Literal::Integer(i64, Option<IntSuffix>)`: `None` means no suffix was written (contextual inference applies); `Some(s)` means the suffix overrides inference and pins the type.

`FloatSuffix` is a `Copy` enum (`F16`, `BF16`, `F32`, `F64`) carried by `Literal::Float(f64, Option<FloatSuffix>)` with the same semantics: `None` means contextual inference (default `f64`); `Some(s)` pins the float type. Half-precision (`F16`/`BF16`) literals must always carry the suffix — they have no contextual default (§1.2).

## Recent Updates
- 2026-04-18: Added `IntSuffix` enum; changed `Literal::Integer(i64)` → `Literal::Integer(i64, Option<IntSuffix>)` to carry explicit type suffixes from the lexer through to semantic analysis.
- 2026-05-25: Added `FloatSuffix` enum; changed `Literal::Float(f64)` → `Literal::Float(f64, Option<FloatSuffix>)` mirroring the integer-suffix encoding for `1.5f32`/`2.0f64` literals.
- 2026-06-15: Added `Literal::Char(char)` for the `char` primitive type (§1.2).
- 2026-06-16: Extended `FloatSuffix` with `F16`/`BF16` for half-precision literals (`1.5f16`, `0.02bf16`, §1.2).
