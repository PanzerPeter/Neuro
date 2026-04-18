# lexical-analysis

## Purpose
Transform raw NEURO source text into a validated token stream as the first stage of the compiler pipeline.

## Entry Point
- Type: Library function
- Input: `source: &str`
- Output: `Result<Vec<Token>, LexError>`

## Data Ownership
- Tables: none
- Events Published: none
- Events Consumed: none
- Public Read Model: none

## Shared Kernel
- shared-types — provides `Span` for byte-range tracking on every token
- diagnostics — error type infrastructure used by `LexError`

## Notes
Logos-generated lexer handles UTF-8 source via XID_Start/XID_Continue rules so
Unicode identifiers are accepted without a hand-written scanner.
`classify_error` exists because Logos surfaces all unrecognised input as a generic
error; reclassifying to `UnterminatedString` gives the diagnostic layer a precise,
actionable error kind.

Compound assignment tokens (`PlusEqual`, `MinusEqual`, `StarEqual`, `SlashEqual`,
`PercentEqual`) are declared alongside arithmetic operators. Logos uses longest-match,
so `+=` is always consumed as a single token rather than `+` then `=`.

`TokenKind::Const` was added as a reserved keyword for compile-time constant declarations
(`const NAME: Type = expr`). It sits between `Mut` and `As` in declaration order.

## Recent Updates
- 2026-04-16: Added `TokenKind::Const` keyword token for §1.3 const declarations.
- 2026-04-18: Added bitwise operator tokens for §1.4: `Pipe` (`|`), `Caret` (`^`), `Tilde` (`~`), `LeftShift` (`<<`). `Amp` (`&`) was already present. `LeftShift` is declared before `Less` so logos longest-match always picks `<<` over `<`.
