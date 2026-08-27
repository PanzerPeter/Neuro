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

`TokenKind::Char(char)` lexes a single-quoted character literal. The regex admits
exactly one content unit — a plain char, a recognized escape (`\n`/`\t`/`\r`/`\\`/`\'`/`\0`),
a `\u{...}` unicode escape, or a `\xNN` byte escape — so `''`, `'ab'`, and an unterminated
`'a` never match and become lex errors. `parse_char` decodes the escape and validates the
`\u{...}` scalar range, emitting `LexError::InvalidCharLiteral` on an out-of-range code point.

A triple-quoted (block) string `"""…"""` produces the same `TokenKind::String` payload
as an ordinary literal, so nothing downstream of the lexer distinguishes the two forms.
It is declared as a bare `#[token("\"\"\"")]` whose callback scans and `bump`s the body
itself: logos has no non-greedy repetition, so a regex ending in `"""` would run to the
LAST `"""` in the file. Three quotes always outrank the two-quote empty-string match under
logos' longest-match rule, so `""` and `"""` never collide.

`dedent_block_body` strips the closing delimiter's whitespace prefix from every content
line. It does so by dropping characters from the indexed `(absolute_offset, char)` vector
rather than by rebuilding a `String`, which is why interpolation holes inside a block
string still report at real source columns — `decode_chunks` slices each hole straight out
of the original source by absolute offset. Rules: the newline after the opening `"""` is
punctuation and is dropped; text trailing the opening `"""` on the same line is content
and is exempt from the dedent check (it sits flush against the delimiter and cannot carry
indentation); a whitespace-only line normalizes to empty without an indent check; any other
line not starting with the closing prefix is `TripleQuoteUnderIndented`. A closing `"""`
that is not alone on its line is `TripleQuoteClosingNotOnOwnLine`, and a body with no
closing delimiter is `UnterminatedTripleQuotedString`.

`TokenKind::String` carries a `StringValue`, not a bare `String`: a literal with no
`{...}` hole decodes to `StringValue::Plain`, one with holes to `StringValue::Interp`
carrying `InterpChunk`s. One token variant covers both because a logos callback picks a
variant's *payload*, never the variant — the decoder decides plain-vs-interpolated only
after walking the content. `decode_string_literal` splits text from holes, decoding
escapes (`\{` writes a literal brace) and recording each hole's raw source with its
**absolute** file span so the parser's sub-parse of the hole reports at real columns.
Hole bounds are found by brace-depth matching that skips char literals, so a `\u{...}`
payload's brace does not close the hole. A `"` inside a hole is not supported: the quote
ends the string token, and the result reports as `LexError::UnterminatedInterpolation`.

A block comment is a bare `#[token("/*")]` whose callback counts nesting depth over
`lex.remainder()` and returns `logos::FilterResult::Skip` once depth returns to zero, or
`FilterResult::Error(UnterminatedBlockComment)` when the source ends first. A regex cannot
express nesting at all — logos matches the longest run its DFA accepts, so
`/* a /* b */ c */` would close at the FIRST `*/` and leave ` c */` to lex as garbage.
This is why `Lexer::classify_error` no longer rewrites a failure sitting on `/*`: the
callback raises the error itself, with the span running from the opening delimiter to EOF.
A comment body is scanned as raw text — a `/*` or `*/` inside a string or char literal
within the comment still counts toward depth, the same way `//` already swallows a quote
to end of line.

### Editor grammar sync
`neuro-language-support/syntaxes/neuro.tmLanguage.json` re-describes this lexer as TextMate
regexes for editor highlighting. Nothing in the build links the two, so `tests/tmlanguage_sync.rs`
is the link: it scans this crate's own `tokens.rs` for `#[token("…")]` literals and fails when a
keyword is missing from the grammar. It covers keyword *words* only — grammar rules with no
one-to-one token counterpart (string bodies and escapes above all) still need updating by hand.
The grammar's `meta.interpolation.neuro` rule already covered `{...}` holes when
interpolation landed, and its `\\.` escape rule already made `\{` an escape, so that
change needed no grammar edit — but a future string-literal change may.
The grammar's block-comment rule is its own `#block_comment` repository entry that
includes only itself, not `#comments`: recursing through `#comments` would let the line
rule `//.*$` inside a block comment consume the `*/` that closes it, which the lexer does
not do.

## Recent Updates
- 2026-08-27: Block comments nest. `_BlockComment` moved from a regex to
  `#[token("/*", lex_nested_block_comment)]` with a depth-counting scanner (see Notes).
  No new `LexError` variant — `UnterminatedBlockComment` already existed and is now
  raised by the callback rather than reconstructed in `Lexer::classify_error`, whose `/*`
  branch was deleted as unreachable. No parser, semantic, or codegen change: comments
  still produce no tokens. The editor grammar needed a hand edit (see Editor grammar sync).
- 2026-08-27: Triple-quoted block strings (`"""…"""`) with dedent. `decode_string_literal`
  now delegates to a shared `decode_chunks` that walks `(absolute_offset, char)` pairs, so
  the new `decode_triple_quoted_string` can dedent by omitting characters while every hole
  keeps a true source span. Three new `LexError` variants:
  `UnterminatedTripleQuotedString`, `TripleQuoteClosingNotOnOwnLine`,
  `TripleQuoteUnderIndented`. No parser, semantic, or codegen change — the token payload is
  unchanged. The editor grammar already carried a `triple_strings` rule ahead of `strings`,
  so it needed no edit.
- 2026-08-25: String interpolation. `TokenKind::String` now carries `StringValue::{Plain, Interp}`
  (see Notes), with `InterpChunk::{Text, Hole}` and the new
  `LexError::UnterminatedInterpolation`. `decode_string_literal` replaced the old
  escape-only decoder; the strict string regex gained `\{` to its escape set.
- 2026-08-24: `/*` with no `*/` is reported as an unterminated block comment.
  `classify_error` already rewrote a logos failure sitting on an opening `"` into
  `UnterminatedString`; the same rewrite now covers `/*`, which previously surfaced as
  "unexpected character '/'". `LexError::UnterminatedBlockComment` existed but had no
  constructor.
- 2026-08-03: Added `TokenKind::Question` (`?`) for the error-propagation operator `expr?`. Declared directly after `QuestionQuestion`; logos' longest-match rule keeps `??` a single coalescing token, so `a ?? b` never lexes as two propagations. Operator token, not a keyword — no TextMate grammar change (`tests/tmlanguage_sync.rs` covers keyword words only).
- 2026-07-24: Added `TokenKind::Move` keyword token for the `move` closure-capture prefix (`move |x| ...`). Reserves the word so it cannot be an identifier. Sits directly after `Unsafe` in declaration order. The word was already present in the editor's TextMate grammar keyword pattern, so `tests/tmlanguage_sync.rs` needed no update.
- 2026-07-19: Added `tests/tmlanguage_sync.rs`, asserting every `#[token("…")]` keyword appears in the editor's TextMate grammar. It caught real drift on introduction: `dyn` was missing from the grammar's keyword pattern, and `f16`/`bf16` from its primitive-type and numeric-suffix patterns.
- 2026-07-19: Added `TokenKind::Dyn` keyword token for `dyn Trait` trait objects. Reserves the word so it cannot be an identifier. Sits directly after `Trait` in declaration order. `impl` needed no new token — the existing `TokenKind::Impl` serves both `impl` blocks and the `impl Trait` bound.
- 2026-07-13: Added `TokenKind::Lifetime(String)` for explicit lifetime names, e.g. `'a` in `func longest<'a>(...)`. Regex `'[_\p{XID_Start}]\p{XID_Continue}*`; the callback strips the leading `'` so the stored name is the bare identifier. A char literal `'a'` carries a closing quote and is a strictly longer match, so logos' longest-match rule keeps char literals winning — only the quote-less form lexes as a lifetime. Sits directly after `Char` in declaration order.
- 2026-07-02: Added `TokenKind::Newtype` keyword token for `newtype Name = T` declarations. Reserves the word so it cannot be an identifier. Sits directly after `Type` in declaration order.
- 2026-07-02: Added `TokenKind::FatArrow` (`=>`) for `match` arms. Sits after `Arrow` in declaration order; logos longest-match keeps `=>` a single token distinct from `=` then `>`.
- 2026-06-09: Added `TokenKind::Loop` keyword token for the `loop { ... }` infinite-loop statement. Reserves the word so it cannot be an identifier. Sits directly after `While` in declaration order.
- 2026-06-04: Added `TokenKind::Unsafe` keyword token for `unsafe { }` blocks (1C groundwork). Reserves the word so it cannot be an identifier. Sits after `Type` in declaration order.
- 2026-06-03: Added `TokenKind::Type` keyword token for type-alias declarations (`type Name = TargetType`). Sits after `Where` in declaration order.
- 2026-04-16: Added `TokenKind::Const` keyword token for const declarations.
- 2026-04-18: Added bitwise operator tokens for: `Pipe` (`|`), `Caret` (`^`), `Tilde` (`~`), `LeftShift` (`<<`). `Amp` (`&`) was already present. `LeftShift` is declared before `Less` so logos longest-match always picks `<<` over `<`.
- 2026-05-18: Added `TokenKind::QuestionQuestion` (`??`) for the null/error coalescing operator (Appendix B row 14). Tokenized now so 1B can lock in R-to-L associativity; full semantics arrive in 1G with Option/Result types.
- 2026-04-18: Added integer literal type suffixes. `IntegerSuffixToken { value: i64, suffix: IntSuffix }` (pub) carries a parsed suffix. `TokenKind::IntegerSuffix(IntegerSuffixToken)` is emitted by four new regexes (decimal, binary, octal, hex each with suffix group `(i8|i16|...|u64)`) at `priority = 2` so logos maximal munch always picks `42i64` as a single token instead of `Integer(42)` + `Identifier("i64")`.
- 2026-05-29: Formalized underscore digit separators. No surface change — every numeric regex already carries `_` in its character class (`[0-9_]*`, `[01_]*`, `[0-7_]*`, `[0-9a-fA-F_]*`) and each `parse_*` helper does `.replace('_', "")` before parsing. Closed out with dedicated cross-base/float/suffixed unit tests; separators are recognized only between digits, so a leading `_` stays an identifier.
- 2026-05-25: Added float literal type suffixes. `FloatSuffixToken { value: f64, suffix: FloatSuffix }` (pub) carries a parsed suffix. `TokenKind::FloatSuffix(FloatSuffixToken)` is emitted by two new regexes (fractional and exponent-only) at `priority = 3` so logos always picks `1.5f32` as a single token instead of `Float(1.5)` + `Identifier("f32")`.
- 2026-06-16: Added half-precision float suffixes `f16`/`bf16`. The two `FloatSuffix` regexes now match `(bf16|f16|f32|f64)`; the suffix is no longer fixed-length, so `parse_fractional_float_suffix` splits it via `split_float_suffix` (which tests `bf16` before `f16`, since `…bf16` also ends in `f16`).
