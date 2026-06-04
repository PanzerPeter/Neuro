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
- 2026-06-04: Added `TokenKind::Unsafe` keyword token for `unsafe { }` blocks (Phase 1.7 groundwork). Reserves the word so it cannot be an identifier. Sits after `Type` in declaration order.
- 2026-06-03: Added `TokenKind::Type` keyword token for §3.14 type-alias declarations (`type Name = TargetType`). Sits after `Where` in declaration order.
- 2026-04-16: Added `TokenKind::Const` keyword token for §1.3 const declarations.
- 2026-04-18: Added bitwise operator tokens for §1.4: `Pipe` (`|`), `Caret` (`^`), `Tilde` (`~`), `LeftShift` (`<<`). `Amp` (`&`) was already present. `LeftShift` is declared before `Less` so logos longest-match always picks `<<` over `<`.
- 2026-05-18: Added `TokenKind::QuestionQuestion` (`??`) for the null/error coalescing operator (§3.11, Appendix B row 14). Tokenized now so Phase 1.5 can lock in R-to-L associativity; full semantics arrive in Phase 2 with Option/Result types.
- 2026-04-18: Added integer literal type suffixes §1.4. `IntegerSuffixToken { value: i64, suffix: IntSuffix }` (pub) carries a parsed suffix. `TokenKind::IntegerSuffix(IntegerSuffixToken)` is emitted by four new regexes (decimal, binary, octal, hex each with suffix group `(i8|i16|...|u64)`) at `priority = 2` so logos maximal munch always picks `42i64` as a single token instead of `Integer(42)` + `Identifier("i64")`.
- 2026-05-29: Formalized underscore digit separators §1.2. No surface change — every numeric regex already carries `_` in its character class (`[0-9_]*`, `[01_]*`, `[0-7_]*`, `[0-9a-fA-F_]*`) and each `parse_*` helper does `.replace('_', "")` before parsing. Closed out with dedicated cross-base/float/suffixed unit tests; separators are recognized only between digits, so a leading `_` stays an identifier.
- 2026-05-25: Added float literal type suffixes §1.2/§1.4. `FloatSuffixToken { value: f64, suffix: FloatSuffix }` (pub) carries a parsed suffix. `TokenKind::FloatSuffix(FloatSuffixToken)` is emitted by two new regexes (fractional and exponent-only) at `priority = 3` so logos always picks `1.5f32` as a single token instead of `Float(1.5)` + `Identifier("f32")`. The suffix is always the trailing three characters (`f32` or `f64`).
