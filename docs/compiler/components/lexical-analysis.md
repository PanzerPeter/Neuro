# Lexical Analysis

**Status**: ✅ Complete (Phase 1)
**Crate**: `compiler/lexical-analysis`
**Entry Point**: `pub fn tokenize(input: &str) -> Result<Vec<Token>, LexError>`

## Overview

The lexical analysis feature slice is responsible for converting raw NEURO source code into a stream of tokens. It implements a complete lexer with Unicode support, multiple number bases, string literals with escape sequences, and comprehensive error reporting.

## Architecture

This slice follows the **Vertical Slice Architecture** pattern:
- **Self-contained**: No dependencies on other feature slices
- **Infrastructure only**: Depends only on `shared-types` for common types
- **Public API**: Single entry point (`tokenize`)
- **Internal implementation**: All internals are `pub(crate)`

## Features

### Token Types Supported

#### Keywords
- `func` - Function definitions
- `val` - Immutable variable declarations
- `mut` - Mutable variable declarations
- `if` / `else` - Conditional statements
- `return` - Return statements
- `true` / `false` - Boolean literals

#### Operators
- **Arithmetic**: `+`, `-`, `*`, `/`, `%`
- **Comparison**: `==`, `!=`, `<`, `>`, `<=`, `>=`
- **Logical**: `&&`, `||`, `!`
- **Assignment**: `=`

#### Delimiters
- `(`, `)` - Parentheses
- `{`, `}` - Braces
- `,` - Comma
- `:` - Colon
- `->` - Arrow (function return type)

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
```

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
    UnexpectedCharacter { ch: char, span: Span },
    UnterminatedString { span: Span },
    InvalidEscape { escape: String, span: Span },
    InvalidNumber { text: String, span: Span },
    InvalidUnicodeEscape { value: String, span: Span },
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

**Test coverage**: 28 comprehensive tests

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

NEURO embraces Unicode for identifiers to support international developers:
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
/// Tokenize a NEURO source file into a token stream
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

    // Literals
    Integer(i64),
    Float(f64),
    String(String),
    Boolean(bool),

    // Identifiers
    Identifier(String),

    // Operators (many variants)...
}
```

## Integration Points

### Downstream Consumers

- **syntax-parsing**: Consumes token stream for AST generation
- **LSP server** (Phase 7): Uses tokens for syntax highlighting

### Dependencies

- **shared-types**: `Span` type for source locations
- No dependencies on other feature slices (maintains slice independence)

## Future Enhancements (Post-Phase 1)

- [ ] Token stream caching for incremental compilation
- [ ] Better error recovery (continue lexing after errors)
- [ ] Preprocessor directives (if needed)
- [ ] Attribute tokens (`@gpu`, `@inline`, etc.)
- [ ] Documentation comment tokens (`///`, `/**`)

## Maintenance

### Adding New Keywords

1. Add keyword to `TokenKind` enum
2. Add logos pattern in lexer implementation
3. Update tests
4. Update documentation

### Adding New Operators

1. Add operator variant to `TokenKind`
2. Add logos pattern (ensure correct precedence)
3. Update operator precedence in parser (downstream)
4. Add tests

## Troubleshooting

### "Unexpected character" errors

**Problem**: Source contains character not recognized by lexer

**Solution**:
- Check for invisible Unicode characters
- Ensure file encoding is UTF-8
- Verify character is valid in NEURO syntax

### "Invalid escape sequence" in strings

**Problem**: String contains unrecognized escape like `\q`

**Solution**:
- Use supported escapes: `\n \r \t \" \\ \0 \xNN \u{NNNN}`
- Or use raw strings (future feature)

## References

- [Logos documentation](https://docs.rs/logos/)
- [Unicode UAX#31](https://unicode.org/reports/tr31/)
- Source: [compiler/lexical-analysis/src/lib.rs](../../compiler/lexical-analysis/src/lib.rs)
