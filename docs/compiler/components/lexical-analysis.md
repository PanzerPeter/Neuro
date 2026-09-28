# Lexical Analysis

**Crate**: `compiler/lexical-analysis`
**Entry Point**: `pub fn tokenize(input: &str) -> Result<Vec<Token>, LexError>`

## Overview

Lexical analysis turns source text into a token stream with a byte span on every token. It is
built on [logos](https://crates.io/crates/logos), with hand-written callbacks for the parts a
regular expression cannot express: string escapes and interpolation holes, triple-quoted
blocks, and nesting block comments. The parser calls `tokenize` itself, so no other stage uses
this crate directly.

## Architecture

- **Dependencies**: `shared-types` (`Span`), `logos`, `thiserror`. No feature slice.
- **Public API**: `tokenize`, `Token`, `TokenKind`, `StringValue`, `InterpChunk`, the literal
  suffix tokens, and `LexError`. Everything else is private.

## Behavior

### Tokens

The token set is [`TokenKind` in `tokens/mod.rs`](../../../compiler/lexical-analysis/src/tokens/mod.rs).
Two things about it are easy to get wrong:

- Type names (`i32`, `f64`, `bool`, `string`, `char`, …) are ordinary identifiers, not
  keywords. The type checker resolves them, which is what lets a `newtype` or a `type` alias
  introduce one.
- An integer token carries the literal's **magnitude** as a `u64`. A literal is never negative
  in source (`-1` is a negation of `1`), and deciding what a magnitude means for a given type
  is the type checker's job.

Integer literals may be decimal, `0x` hex, `0o` octal or `0b` binary, with `_` digit separators
and an optional type suffix (`42u8`); floats take an exponent and an `f16` / `bf16` / `f32` /
`f64` suffix. Identifiers follow Unicode XID rules (`计算` and `café` are identifiers). The
user-facing rules are in the [types reference](../../language-reference/types.md), and string
escapes in the [strings reference](../../language-reference/strings.md#escape-sequences).

### Strings and interpolation

A string token carries a [`StringValue`](../../../compiler/lexical-analysis/src/tokens/mod.rs):
`Plain(String)` for a literal without holes, `Interp(Vec<InterpChunk>)` when it contains at
least one `{expr}` hole. Each chunk is either `Text(String)` (already unescaped) or
`Hole { source, span }` (the raw expression text plus its location). The parser hands the
chunks to expression parsing; see
[string interpolation](../../language-reference/expressions.md#string-interpolation) for the
user-facing syntax and format mini-language. An unterminated `{` hole is the lexer's
`UnterminatedInterpolation` error.

A triple-quoted `"""…"""` block string decodes to the same `StringValue`, so nothing
downstream of the lexer distinguishes the two forms. Logos matches only the opening
delimiter. It has no non-greedy repetition, so a regex ending in `"""` would run to the
last one in the file. A callback scans the body, strips the closing delimiter's
indentation, and reuses the ordinary chunk decoder. Dedent drops characters from an
indexed `(offset, char)` view rather than rebuilding the text, which is how holes inside a
block string keep true source spans. See
[triple-quoted strings](../../language-reference/expressions.md#triple-quoted-strings) for
the dedent rules and their errors.

### Operators

- **Arithmetic**: `+`, `-`, `*`, `/`, `%`
- **Compound assignment**: `+=`, `-=`, `*=`, `/=`, `%=`
- **Comparison**: `==`, `!=`, `<`, `>`, `<=`, `>=`
- **Logical**: `&&`, `||`, `!`
- **Bitwise**: `&`, `|`, `^`, `~`, `<<` (right shift is the `.shr(n)` method, because
  `>>` spells function composition)
- **Composition**: `>>` is not a token of its own. It reaches the parser as two adjacent
  `>`, which is what keeps the closing brackets of `Vec<Vec<i32>>` two tokens the type
  parser can consume one at a time.
- **Fallible**: `??` (coalesce), `?` (propagate)
- **Assignment**: `=`
- **Other**: `@` (attributes and matrix multiplication), `->` (return type), `=>` (match arm), `::` (path and
  turbofish), `..` / `..=` (ranges), `.` (member access)

### Delimiters and newlines

`(` `)` · `{` `}` · `[` `]` · `,` · `:` · `;`

`;` is tokenized only so a stray semicolon can be reported as an unexpected token; Neuro
statements are newline-terminated. Newlines are themselves tokens (`TokenKind::Newline`),
because the parser needs them to find statement boundaries.

### Comments

```neuro
// Line comment

/*
 * Block comment
 * Can span multiple lines
 */

/* Block comments nest: /* this inner one */ and the outer is still open. */
```

Nesting means a block comment ends only at the `*/` that unwinds it to depth zero,
so a block already containing a comment can be commented out wholesale. Each `/*`
therefore needs its own `*/`; a file that ends while a comment is still open is
`LexError::UnterminatedBlockComment`. A comment body is raw text: `/*` and `*/`
inside a string or char literal within it are still counted, exactly as `//`
already swallows a quote to end of line.

## Errors

Every lexical failure is a `LexError` variant carrying the `Span` of the offending text: an
unexpected character, an unterminated string, block comment, interpolation hole or
triple-quoted block, and malformed numbers, escapes and character literals. The authoritative
list is [`errors.rs`](../../../compiler/lexical-analysis/src/errors.rs). Lexing stops at the
first error; error recovery is planned on the roadmap.

## Keeping the editor grammar in sync

Nothing links `TokenKind` to `neuro-language-support/syntaxes/neuro.tmLanguage.json`, so any
change to the token set has to update that editor grammar by hand in the same commit.
`tests/tmlanguage_sync.rs` checks what it can without a tokenizer: that every keyword is
covered, that the grammar's keyword rule invents none of its own, and that the rules
naming a declaration are ordered ahead of the keyword rule so they stay reachable.
`tools/tmlanguage_scopes.mjs` prints the scopes the grammar actually assigns to a source
file, for the rules those checks cannot reach.

## Source

- [`compiler/lexical-analysis/src/lib.rs`](../../../compiler/lexical-analysis/src/lib.rs)
- [`compiler/lexical-analysis/CONTEXT.md`](../../../compiler/lexical-analysis/CONTEXT.md)

## See Also

- [Syntax Parsing](syntax-parsing.md)
- [Editor Support](../../guides/editor-support.md)
