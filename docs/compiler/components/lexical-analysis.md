# Lexical Analysis

**Crate**: `compiler/lexical-analysis`
**Entry Point**: `pub fn tokenize(source: &str) -> LexResult<Vec<Token>>`

The lexer turns Neuro source text into a token stream, each token carrying the byte span it was read from. Only the parser calls it, so read this page when you change the token set, a literal format, or an escape.

## Overview

Most tokens are plain regular expressions in a [logos](https://crates.io/crates/logos) lexer. Hand-written callbacks cover what a regular expression cannot express: nested block comments, triple-quoted strings, string escapes and interpolation holes, and the split of `t.0.1` into tuple indices. The stream always ends with an `Eof` token. The first lexical error stops the scan.

## Architecture

- **Dependencies**: `shared-types` (`Span`), `logos`, `thiserror`. No feature slice.
- **Public API**: `tokenize`, `LexResult`, `LexError`, `Token`, `TokenKind`, `StringValue`, `InterpChunk`, and the integer and float suffix tokens. Everything else is private.
- **Files**: [`tokens/mod.rs`](../../../compiler/lexical-analysis/src/tokens/mod.rs) defines the token kinds and attaches their patterns. `tokens/numbers.rs` and `tokens/strings.rs` decode literal payloads. `errors.rs` defines `LexError`. `lib.rs` runs the scan and reclassifies the generic logos error.

## Behavior

### Identifiers and numbers

- Identifiers follow Unicode XID rules, so `计算` and `café` are identifiers.
- Keywords reserve their word. Type names such as `i32`, `f64`, `bool`, `string` and `char` are ordinary identifiers, which lets a `type` alias or a `newtype` introduce one. The type checker resolves them.
- Integers are decimal, `0x` hex, `0o` octal or `0b` binary. `_` separates digits and never leads. An optional suffix from `i8` to `u64` attaches to the literal, as in `42u8` or `0xFFu8`.
- A float has a fractional part, an exponent, or both (`1.5`, `1e3`, `1.5e-2`), and takes an optional `f16`, `bf16`, `f32` or `f64` suffix. A suffix needs one of those forms. `42f32` lexes as an integer followed by an identifier, and the parser rejects it.
- An integer token carries its **magnitude** as a `u64`. Source never spells a negative literal (`-1` is `-` applied to `1`), so the lexer never produces a negative payload. What a magnitude means for a type is the checker's call.
- `t.0.1` lexes as two tuple indices, not as the float `0.1`. Only a plain `digits.digits` directly after a `.` is split. A float with an exponent is left whole, and the parser reports it.

The user-facing rules are in the [types reference](../../language-reference/types.md).

### Operators and newlines

- A compound assignment (`+=`, `<<=`, and the rest) is one token, never an operator followed by `=`. Longest match makes `<<=` win over `<<` and `<=`.
- `??` and `?` are distinct tokens, and `|>` is a token of its own, distinct from `|` and `||`.
- `>>` is not a token. It reaches the parser as two adjacent `>`, which keeps the closing brackets of `Vec<Vec<i32>>` separate. The parser reads the adjacent pair as composition. Right shift is the `.shr(n)` method, and `<<` is the only shift token.
- `;` is tokenized so that a stray one is reported as an unexpected token. Neuro statements end at newlines, and runs of newlines collapse into one `Newline` token that the parser uses to find statement boundaries.

### Strings and interpolation

A string token carries a `StringValue`. A literal with no `{...}` hole is plain. Otherwise its chunks are either decoded text or a hole that keeps its raw source and span, for the parser to read as an expression. See [string interpolation](../../language-reference/strings.md#string-interpolation). The escape list and the `\{` and `\}` brace escapes are in the [strings reference](../../language-reference/strings.md#escape-sequences).

An ordinary literal cannot span lines. An unclosed `"` ends the scan with `UnterminatedString`, reported on its own line. A `"` inside a hole ends the literal too, and the lexer reports the hole as unterminated.

A triple-quoted `"""` block decodes to the same `StringValue`, so nothing downstream tells the two forms apart. Logos matches only the opening delimiter. A regex ending in `"""` would run to the last `"""` in the file, because logos has no non-greedy repetition. A callback scans the body instead, strips the closing delimiter's indentation, and keeps each hole at its true source position. The dedent rules and their errors are in [triple-quoted strings](../../language-reference/strings.md#triple-quoted-strings).

### Characters and lifetimes

A character literal holds exactly one scalar: a plain character, one of the escapes `\n`, `\r`, `\t`, `\\`, `\'` and `\0`, a `\xNN` byte escape, or a `\u{...}` escape. An out-of-range `\u{...}` is `InvalidCharLiteral`. A quote followed by an identifier and no closing quote is a lifetime, so `'a'` is a character and `'a` is a lifetime.

### Comments

A `//` comment runs to the end of its line. A `/* */` comment nests:

```neuro
// Line comment

/*
 * Block comment
 * Can span multiple lines
 */

/* Block comments nest: /* this inner one */ and the outer is still open. */
```

A block comment ends at the `*/` that returns the depth to zero, so a block that already holds a comment can be commented out whole. Each `/*` needs its own `*/`. The body is raw text: a `/*` or `*/` inside a string or character literal in the comment still counts, as a quote inside a line comment is swallowed to the end of the line.

## Diagnostics

Every lexical failure is a `LexError` carrying the `Span` of the offending text, and `LexError::span()` returns it. The driver prints the message with a location, as a parse error:

| Source | Message |
|---|---|
| `"abc` with no closing quote on its line | `error: lexical error: unterminated string literal starting at position N` |
| `/*` never closed | `unterminated block comment starting at position N` |
| `"v {x"` | `interpolation hole opened with `{` is never closed at position N: add the matching `}` ...` |
| `"a } b"` | `unescaped `}` at position N is outside an interpolation hole: write `\}` ...` |
| `"bad \q"` | `invalid escape sequence '\q' at position N` |
| `$` outside any token | `unexpected character '$' at position N` |
| A closing `"""` that shares its line with other text | `closing `"""` at position N must be on its own line ...` |

Positions are byte offsets into the file. The lexer stops at the first error, and error recovery is on the [Quick Roadmap](../../../README.md#quick-roadmap).

## Testing

- [`compiler/lexical-analysis/src/tests.rs`](../../../compiler/lexical-analysis/src/tests.rs) holds the scanner's unit tests.
- [`compiler/lexical-analysis/tests/tmlanguage_sync.rs`](../../../compiler/lexical-analysis/tests/tmlanguage_sync.rs) checks the editor grammar against the token set.
- Literal behaviour end to end is covered by the `compiler/neurc/tests/suite/` files for integer and float suffixes, underscore separators, string interpolation, triple-quoted strings and nested block comments.

Nothing in the build links `TokenKind` to `neuro-language-support/syntaxes/neuro.tmLanguage.json`, so a token change must update the editor grammar by hand in the same change. The test checks that every keyword appears in the grammar, that the grammar's keyword rule lists no word the lexer does not tokenize, and that declaration rules come before the keyword rule. String bodies, escapes, holes and numeric shapes have no one-to-one token, so the test does not cover them. [`tools/tmlanguage_scopes.mjs`](../../../tools/tmlanguage_scopes.mjs) prints the scopes the grammar assigns to a file, for checking those by eye.

## Source

- [`compiler/lexical-analysis/src/lib.rs`](../../../compiler/lexical-analysis/src/lib.rs)
- [`compiler/lexical-analysis/CONTEXT.md`](../../../compiler/lexical-analysis/CONTEXT.md): declaration-order ties, literal decoding details, and the grammar sync rules

## See Also

- [Syntax Parsing](syntax-parsing.md)
- [Editor Support](../../guides/editor-support.md)
