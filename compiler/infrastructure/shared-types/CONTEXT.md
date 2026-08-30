# shared-types

## Purpose
Provide the lightweight, zero-business-logic data structures used universally across compiler slices: source spans, identifiers, literal values, and format specifiers.

## Entry Point
- Type: Library (no entry function — pure data)
- Public types: `Span`, `Identifier`, `Literal`, `IntSuffix`, `FloatSuffix`, `FormatSpec`,
  `FormatAlign`, `FormatKind`

## Data Ownership
- Tables / Events Published / Events Consumed / Public Read Model: none

## Shared Kernel
None. This is the lowest-level crate in the workspace.

## Notes
`Span` is a half-open byte-offset range `[start, end)` carried by every token and AST node, so
a diagnostic can point at exact source. `Identifier` pairs a `String` name with a `Span`.

`Literal` enumerates every compile-time constant kind. Three of them carry an **optional**
suffix, and `None` vs `Some` is the whole inference contract:
- `Literal::Integer(i64, Option<IntSuffix>)` — `IntSuffix` is a `Copy` enum of the eight
  integer suffixes (`I8`–`U64`). `None` means no suffix was written and contextual inference
  applies; `Some(s)` pins the type and overrides inference.
- `Literal::Float(f64, Option<FloatSuffix>)` — `FloatSuffix` is `F16` / `BF16` / `F32` / `F64`,
  same semantics (`None` defaults to `f64`). Half-precision literals must always carry their
  suffix: they have no contextual default.
- `Literal::Char(char)` holds one Unicode scalar value.

`FormatSpec` (with `FormatAlign` / `FormatKind`) is the parsed `spec` half of a
string-interpolation hole `{expr:spec}`, alongside the `MAX_FORMAT_WIDTH` and
`MAX_FORMAT_PRECISION` ceilings semantic analysis enforces. It is pure data with the
validation split three ways: the parser checks the written grammar, the type checker checks it
against the value's type, and every pass between them only reads fields.
