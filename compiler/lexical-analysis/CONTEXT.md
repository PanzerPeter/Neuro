# lexical-analysis

## Purpose
Transform raw Neuro source text into a validated token stream as the first stage of the compiler pipeline.

## Entry Point
- Type: Library function
- Input: `source: &str`
- Output: `Result<Vec<Token>, LexError>`

## Data Ownership
- Tables / Events Published / Events Consumed / Public Read Model: none

## Shared Kernel
- shared-types — `Span` for the byte range on every token, plus `IntSuffix` / `FloatSuffix`
- diagnostics — error type infrastructure used by `LexError`

## Notes
A logos-generated lexer over UTF-8 source, using XID_Start / XID_Continue rules so Unicode
identifiers are accepted without a hand-written scanner. Every keyword token exists to reserve
its word against use as an identifier.

`classify_error` exists because logos surfaces all unrecognised input as one generic error;
reclassifying it (to `UnterminatedString`, for instance) gives the diagnostic layer a precise,
actionable kind.

### Declaration order and longest-match ties
Logos resolves overlaps by longest match, then by declaration order and explicit `priority`.
Several tokens depend on that and will silently regress if reordered:
- `+=` / `-=` / `*=` / `/=` / `%=` are single tokens, never operator-then-`=`.
- `<<` is declared before `<`; `=>` is distinct from `=` then `>`; `??` stays one coalescing
  token, so `a ?? b` never lexes as two `?` propagations.
- `TokenKind::IntegerSuffix(IntegerSuffixToken)` is emitted by four regexes (decimal, binary,
  octal, hex, each with a `(i8|…|u64)` suffix group) at `priority = 2`, and
  `TokenKind::FloatSuffix(FloatSuffixToken)` by two (fractional and exponent-only) matching
  `(bf16|f16|f32|f64)` at `priority = 3` — so `42i64` and `1.5f32` are one token rather than a
  literal followed by an identifier. Because the float suffix is no longer fixed-length,
  `split_float_suffix` tests `bf16` before `f16`: `…bf16` also ends in `f16`.
- `TokenKind::Lifetime(String)` matches `'[_\p{XID_Start}]\p{XID_Continue}*` with the callback
  stripping the leading `'`. A char literal carries a closing quote and is therefore a strictly
  longer match, which is the only thing keeping `'a'` a char and `'a` a lifetime.
- `"""` always outranks the two-quote empty-string match, so `""` and `"""` never collide.

Underscore digit separators need no separate rule: every numeric regex carries `_` in its
character class (`[0-9_]*`, `[01_]*`, `[0-7_]*`, `[0-9a-fA-F_]*`) and each `parse_*` helper
strips them before parsing. Separators are recognized only *between* digits, so a leading `_`
stays an identifier.

### Character literals
`TokenKind::Char(char)`'s regex admits exactly one content unit — a plain char, a recognized
escape (`\n`/`\t`/`\r`/`\\`/`\'`/`\0`), a `\u{...}` unicode escape, or a `\xNN` byte escape — so
`''`, `'ab'`, and an unterminated `'a` never match and become lex errors. `parse_char` decodes
the escape and validates the `\u{...}` scalar range, emitting `LexError::InvalidCharLiteral` on
an out-of-range code point.

### Strings
`TokenKind::String` carries a `StringValue`, not a bare `String`: a literal with no `{...}` hole
decodes to `StringValue::Plain`, one with holes to `StringValue::Interp` carrying `InterpChunk`s.
**One** token variant covers both because a logos callback picks a variant's *payload*, never the
variant — the decoder can only decide plain-vs-interpolated after walking the content.

`decode_string_literal` splits text from holes, decoding escapes (`\{` / `\}` write literal
braces; an unescaped `}` outside a hole is `LexError::UnescapedClosingBrace`)
and recording each hole's raw source with its **absolute** file span, so the parser's sub-parse
of the hole reports at real columns. Hole bounds come from brace-depth matching that skips char
literals, so a `\u{...}` payload's brace does not close the hole. A `"` inside a hole is not
supported — the quote ends the string token — and surfaces as
`LexError::UnterminatedInterpolation`.

A triple-quoted (block) string `"""…"""` produces the same `TokenKind::String` payload as an
ordinary literal, so nothing downstream distinguishes the two forms. It is declared as a bare
`#[token("\"\"\"")]` whose callback scans and `bump`s the body itself: logos has no non-greedy
repetition, so a regex ending in `"""` would run to the LAST `"""` in the file.

`dedent_block_body` strips the closing delimiter's whitespace prefix from every content line. It
does so by **dropping characters from the indexed `(absolute_offset, char)` vector** rather than
rebuilding a `String`, which is why interpolation holes inside a block string still report at
real source columns — `decode_chunks` slices each hole straight out of the original source by
absolute offset. Its rules:
- the newline after the opening `"""` is punctuation and is dropped, and so is the newline
  before the closing delimiter's line — each content line pushes its own terminator, so the
  last of those is popped after the walk. A trailing newline is written as a blank line
  before the closer, whose terminator then becomes the surviving one;
- text trailing the opening `"""` on the same line is content, exempt from the dedent check (it
  sits flush against the delimiter and cannot carry indentation);
- a whitespace-only line normalizes to empty with no indent check;
- any other line not starting with the closing prefix is `TripleQuoteUnderIndented`;
- a line's trailing `\r` is line-ending punctuation and is dropped with its newline, so a CRLF
  source (a Windows checkout under git's `core.autocrlf`) yields byte-identical values to an LF
  one;
- a closing `"""` not alone on its line is `TripleQuoteClosingNotOnOwnLine`, and a body with no
  closing delimiter is `UnterminatedTripleQuotedString`.

### Block comments nest
A block comment is a bare `#[token("/*")]` whose callback counts nesting depth over
`lex.remainder()`, returning `logos::FilterResult::Skip` once depth returns to zero or
`FilterResult::Error(UnterminatedBlockComment)` when the source ends first. A regex cannot
express nesting at all — logos matches the longest run its DFA accepts, so `/* a /* b */ c */`
would close at the FIRST `*/` and leave ` c */` to lex as garbage. This is also why
`classify_error` does not rewrite a failure sitting on `/*`: the callback raises that error
itself, spanning the opening delimiter to EOF. A comment body is scanned as raw text — a `/*` or
`*/` inside a string or char literal within the comment still counts toward depth, the same way
`//` already swallows a quote to end of line.

### Editor grammar sync
`neuro-language-support/syntaxes/neuro.tmLanguage.json` re-describes this lexer as TextMate
regexes for editor highlighting. **Nothing in the build links the two**, so
`tests/tmlanguage_sync.rs` is the link. It asserts three properties, each standing for a bug
class the grammar has shipped:

- **Coverage** — every `#[token("…")]` keyword appears somewhere in the grammar.
- **No invention** — the grammar's `#keywords` rule lists *only* words this lexer tokenizes, so a
  later phase's vocabulary cannot be highlighted as if the compiler already accepted it.
- **Reachability** — the rules naming a declaration (`#function_declaration`,
  `#type_declarations`, `#imports`) precede `#keywords` in the top-level `patterns` array, and
  `#chars` precedes `#lifetimes`. TextMate resolves a tie between two rules matching at the same
  offset by array position, never by match length, so a declaration rule listed after `#keywords`
  is dead code that nothing about the file reveals on inspection.

The tests cover keyword words and rule order only. Rules with no one-to-one token counterpart —
string bodies, escape sets, interpolation holes, numeric literal shapes — still need updating by
hand, and the names in the grammar's `#types` and `#constants` answer to the prelude and the type
checker rather than to this crate. To see what the grammar actually produces for a source file,
run `tools/tmlanguage_scopes.mjs`, which drives the same tokenizer VS Code uses; it is
deliberately outside CI so the workspace keeps no Node dependency.

Three grammar rules are load-bearing and easy to break:
- the escape rule must match `\u{...}` and `\xNN` **whole** — matching a bare `\\.` leaves the
  brace of `\u{1F600}` to the interpolation rule, which then reads the codepoint as a `{expr}` hole;
- the interpolation rule does not include `$self`, because a hole may not contain a `"` string
  literal;
- the block-comment rule is its own `#block_comment` repository entry that includes only *itself*,
  not `#comments` — recursing through `#comments` would let the line rule `//.*$` inside a block
  comment consume the `*/` that closes it, which the lexer does not do.
