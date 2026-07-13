# lexical-analysis

## Purpose
Transform raw Neuro source text into a validated token stream as the first stage of the compiler pipeline.

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

`TokenKind::Char(char)` lexes a single-quoted character literal (§1.2). The regex admits
exactly one content unit — a plain char, a recognized escape (`\n`/`\t`/`\r`/`\\`/`\'`/`\0`),
a `\u{...}` unicode escape, or a `\xNN` byte escape — so `''`, `'ab'`, and an unterminated
`'a` never match and become lex errors. `parse_char` decodes the escape and validates the
`\u{...}` scalar range, emitting `LexError::InvalidCharLiteral` on an out-of-range code point.

## Recent Updates
- 2026-07-13: Added `TokenKind::Lifetime(String)` for explicit lifetime names (§2.6), e.g. `'a` in `func longest<'a>(...)`. Regex `'[_\p{XID_Start}]\p{XID_Continue}*`; the callback strips the leading `'` so the stored name is the bare identifier. A char literal `'a'` carries a closing quote and is a strictly longer match, so logos' longest-match rule keeps char literals winning — only the quote-less form lexes as a lifetime. Sits directly after `Char` in declaration order.
- 2026-07-02: Added `TokenKind::Newtype` keyword token for `newtype Name = T` declarations (§3.15). Reserves the word so it cannot be an identifier. Sits directly after `Type` in declaration order.
- 2026-07-02: Added `TokenKind::FatArrow` (`=>`) for `match` arms (§3.6). Sits after `Arrow` in declaration order; logos longest-match keeps `=>` a single token distinct from `=` then `>`.
- 2026-06-09: Added `TokenKind::Loop` keyword token for the `loop { ... }` infinite-loop statement (§3.7). Reserves the word so it cannot be an identifier. Sits directly after `While` in declaration order.
- 2026-06-04: Added `TokenKind::Unsafe` keyword token for `unsafe { }` blocks (1C groundwork). Reserves the word so it cannot be an identifier. Sits after `Type` in declaration order.
- 2026-06-03: Added `TokenKind::Type` keyword token for §3.14 type-alias declarations (`type Name = TargetType`). Sits after `Where` in declaration order.
- 2026-04-16: Added `TokenKind::Const` keyword token for §1.3 const declarations.
- 2026-04-18: Added bitwise operator tokens for §1.4: `Pipe` (`|`), `Caret` (`^`), `Tilde` (`~`), `LeftShift` (`<<`). `Amp` (`&`) was already present. `LeftShift` is declared before `Less` so logos longest-match always picks `<<` over `<`.
- 2026-05-18: Added `TokenKind::QuestionQuestion` (`??`) for the null/error coalescing operator (§3.11, Appendix B row 14). Tokenized now so 1B can lock in R-to-L associativity; full semantics arrive in 1G with Option/Result types.
- 2026-04-18: Added integer literal type suffixes §1.4. `IntegerSuffixToken { value: i64, suffix: IntSuffix }` (pub) carries a parsed suffix. `TokenKind::IntegerSuffix(IntegerSuffixToken)` is emitted by four new regexes (decimal, binary, octal, hex each with suffix group `(i8|i16|...|u64)`) at `priority = 2` so logos maximal munch always picks `42i64` as a single token instead of `Integer(42)` + `Identifier("i64")`.
- 2026-05-29: Formalized underscore digit separators §1.2. No surface change — every numeric regex already carries `_` in its character class (`[0-9_]*`, `[01_]*`, `[0-7_]*`, `[0-9a-fA-F_]*`) and each `parse_*` helper does `.replace('_', "")` before parsing. Closed out with dedicated cross-base/float/suffixed unit tests; separators are recognized only between digits, so a leading `_` stays an identifier.
- 2026-05-25: Added float literal type suffixes §1.2/§1.4. `FloatSuffixToken { value: f64, suffix: FloatSuffix }` (pub) carries a parsed suffix. `TokenKind::FloatSuffix(FloatSuffixToken)` is emitted by two new regexes (fractional and exponent-only) at `priority = 3` so logos always picks `1.5f32` as a single token instead of `Float(1.5)` + `Identifier("f32")`.
- 2026-06-16: Added half-precision float suffixes `f16`/`bf16` (§1.2). The two `FloatSuffix` regexes now match `(bf16|f16|f32|f64)`; the suffix is no longer fixed-length, so `parse_fractional_float_suffix` splits it via `split_float_suffix` (which tests `bf16` before `f16`, since `…bf16` also ends in `f16`).
