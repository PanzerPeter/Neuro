# Lexical Analysis

**Status**: Complete (Phase 1)
**Crate**: `compiler/lexical-analysis`
**Entry Point**: `pub fn tokenize(input: &str) -> Result<Vec<Token>, LexError>`

## Overview

The lexical analysis feature slice is responsible for converting raw Neuro source code into a stream of tokens. It implements a complete lexer with Unicode support, multiple number bases, string literals with escape sequences, and comprehensive error reporting.

## Architecture

This slice follows the **Vertical Slice Architecture** pattern:
- **Self-contained**: No dependencies on other feature slices
- **Infrastructure only**: Depends only on `shared-types` for common types
- **Public API**: Single entry point (`tokenize`)
- **Internal implementation**: All internals are `pub(crate)`

## Features

### Token Types Supported

#### Keywords

`func` · `val` · `mut` · `const` · `as` · `if` · `else` · `return` · `true` · `false` ·
`while` · `loop` · `for` · `in` · `break` · `continue` · `struct` · `enum` · `impl` ·
`trait` · `dyn` · `import` · `export` · `module` · `match` · `where` · `type` · `newtype` ·
`unsafe` · `move` · `self` · `Self`

Type names (`i32`, `f64`, `bool`, `string`, `char`, …) are ordinary identifiers, not
keywords: they are resolved by the type checker, which is what lets a `newtype` or a
`type` alias introduce one.

#### String Literals and Interpolation

A string token carries a [`StringValue`](../../../compiler/lexical-analysis/src/tokens.rs):
`Plain(String)` for a literal without holes, `Interp(Vec<InterpChunk>)` when it contains at
least one `{expr}` hole. Each chunk is either `Text(String)` (already unescaped) or
`Hole { source, span }` (the raw expression text plus its location). The parser hands the
chunks to expression parsing; see
[string interpolation](../../language-reference/expressions.md#string-interpolation) for the
user-facing syntax and format mini-language. An unterminated `{` hole is the lexer's
`UnterminatedInterpolation` error.

A triple-quoted `"""…"""` block string decodes to the same `StringValue`, so nothing
downstream of the lexer distinguishes the two forms. Logos matches only the opening
delimiter — it has no non-greedy repetition, so a regex ending in `"""` would run to the
last one in the file — and a callback scans the body, strips the closing delimiter's
indentation, and reuses the ordinary chunk decoder. Dedent drops characters from an
indexed `(offset, char)` view rather than rebuilding the text, which is how holes inside a
block string keep true source spans. See
[triple-quoted strings](../../language-reference/expressions.md#triple-quoted-strings) for
the dedent rules and their errors.

#### Operators

- **Arithmetic**: `+`, `-`, `*`, `/`, `%`
- **Compound assignment**: `+=`, `-=`, `*=`, `/=`, `%=`
- **Comparison**: `==`, `!=`, `<`, `>`, `<=`, `>=`
- **Logical**: `&&`, `||`, `!`
- **Bitwise**: `&`, `|`, `^`, `~`, `<<` (there is no `>>` token; right shift is the
  `.shr(n)` method, because `>>` is reserved for function composition)
- **Fallible**: `??` (coalesce), `?` (propagate)
- **Assignment**: `=`
- **Other**: `@` (attributes), `->` (return type), `=>` (match arm), `::` (path and
  turbofish), `..` / `..=` (ranges), `.` (member access)

#### Delimiters

`(` `)` · `{` `}` · `[` `]` · `,` · `:` · `;`

`;` is tokenized only so a stray semicolon can be reported as an unexpected token; Neuro
statements are newline-terminated. Newlines are themselves tokens (`TokenKind::Newline`),
because the parser needs them to find statement boundaries.

#### Literals

**Integers** (multiple bases):
```neuro
42          // Decimal
0b1010      // Binary
0o52        // Octal
0x2A        // Hexadecimal
```

**Floats**:
```neuro
3.14
1.0e10
2.5e-3
```

**Strings** (with escape sequences):
```neuro
"hello world"
"line 1\nline 2"
"tab\there"
"quote: \""
"unicode: \u{1F600}"
"hex: \xAB"
```

**Booleans**:
```neuro
true
false
```

#### Identifiers
- Unicode support (XID_Start + XID_Continue)
- Examples: `myVar`, `_private`, `计算`, `café`

#### Comments
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
`LexError::UnterminatedBlockComment`. A comment body is raw text — `/*` and `*/`
inside a string or char literal within it are still counted, exactly as `//`
already swallows a quote to end of line.

### Span Tracking

Every token includes precise source location information:
```rust
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,  // start and end byte positions
}
```

This enables:
- Accurate error reporting
- IDE features (go-to-definition, hover)
- Debugging information in generated code

## Usage

### Basic Example

```rust
use lexical_analysis::tokenize;

let source = r#"
    func add(a: i32, b: i32) -> i32 {
        return a + b
    }
"#;

let tokens = tokenize(source)?;
for token in tokens {
    println!("{:?} at {:?}", token.kind, token.span);
}
```

### Error Handling

```rust
use lexical_analysis::{tokenize, LexError};

let source = "val x = \"unterminated string";
match tokenize(source) {
    Ok(tokens) => println!("Success: {} tokens", tokens.len()),
    Err(LexError::UnterminatedString { span }) => {
        eprintln!("Error at {:?}: unterminated string", span);
    }
    Err(e) => eprintln!("Lexical error: {}", e),
}
```

## Error Types

```rust
pub enum LexError {
    UnexpectedChar { character: char, span: Span },
    UnterminatedString { span: Span },
    InvalidNumber { text: String, span: Span },
    InvalidEscape { escape: String, span: Span },
    InvalidCharLiteral { literal: String, span: Span },
    UnterminatedBlockComment { span: Span },
    UnterminatedInterpolation { span: Span },
}
```

All errors include span information for precise error reporting.

## Implementation Details

### Technology

- **Lexer generator**: [logos](https://crates.io/crates/logos) 0.14
- **Unicode support**:
  - `unicode-ident` for identifier validation
  - `unicode-segmentation` for string processing

### Performance

- Zero-copy tokenization where possible
- Lazy evaluation of token values
- Efficient string interning for identifiers

### Testing

Test categories:
- Keywords and identifiers
- All operator types
- Number literals (all bases, floats)
- String literals and escape sequences
- Comments (line and block)
- Error cases (invalid syntax, unterminated strings, bad escapes)

Example test:
```rust
#[test]
fn tokenize_string_with_escapes() {
    let input = r#""hello\nworld\t\u{1F600}""#;
    let tokens = tokenize(input).unwrap();
    assert_eq!(tokens.len(), 1);
    match &tokens[0].kind {
        TokenKind::String(s) => {
            assert!(s.contains('\n'));
            assert!(s.contains('\t'));
        }
        _ => panic!("Expected string token"),
    }
}
```

## Design Decisions

### Why logos?

- **Performance**: Generates optimized DFA-based lexer
- **Simplicity**: Declarative regex-based token definitions
- **Maintenance**: Easy to add new token types
- **Error handling**: Integrated error recovery

### Unicode Support

Neuro embraces Unicode for identifiers to support international developers:
- Follows UAX#31 (Unicode Identifier Syntax)
- XID_Start for first character
- XID_Continue for subsequent characters

### String Escape Sequences

Supports common escape sequences for developer convenience:
- `\n`, `\r`, `\t` - Common whitespace
- `\"`, `\\` - Quote and backslash
- `\0` - Null character
- `\xNN` - Hex byte (2 digits)
- `\u{NNNN}` - Unicode codepoint (1-6 hex digits)

## API Reference

### Public Functions

```rust
/// Tokenize a Neuro source file into a token stream
pub fn tokenize(input: &str) -> Result<Vec<Token>, LexError>
```

### Public Types

```rust
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

pub enum TokenKind {
    // Keywords
    Func, Val, Mut, If, Else, Return,

    // Literals (integer and float also have suffixed forms)
    Integer(i64),
    Float(f64),
    String(StringValue),   // Plain(..) or Interp(..), see above
    Char(char),
    Boolean(bool),

    // Identifiers and lifetimes
    Identifier(String),
    Lifetime(String),

    // Operators (many variants)...
}
```

The excerpt above shows representative variants; the full set lives in
[`tokens.rs`](../../../compiler/lexical-analysis/src/tokens.rs).

## Integration Points

### Downstream Consumers

- **syntax-parsing**: Consumes token stream for AST generation
- **LSP server** (Phase 8): Uses tokens for syntax highlighting

### Dependencies

- **shared-types**: `Span` type for source locations
- No dependencies on other feature slices (maintains slice independence)

## Future Enhancements

- [ ] Token stream caching for incremental compilation
- [ ] Better error recovery (continue lexing after an error)
- [ ] Documentation comment tokens (`///`, `/**`)

Nothing links `TokenKind` to `neuro-language-support/syntaxes/neuro.tmLanguage.json`, so any
change to the token set has to update that editor grammar by hand in the same commit.
`tests/tmlanguage_sync.rs` checks what it can without a tokenizer — that every keyword is
covered, that the grammar's keyword rule invents none of its own, and that the rules
naming a declaration are ordered ahead of the keyword rule so they stay reachable.
`tools/tmlanguage_scopes.mjs` prints the scopes the grammar actually assigns to a source
file, for the rules those checks cannot reach.
