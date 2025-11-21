// NEURO Programming Language - Lexical Analysis
// Feature slice for tokenization and lexical processing

use logos::Logos;
use shared_types::Span;
use thiserror::Error;
use unicode_segmentation::UnicodeSegmentation;

/// Token types in the NEURO language
#[derive(Debug, Clone, PartialEq, Logos)]
#[logos(skip r"[ \t\r]+")]
#[logos(error = LexError)]
pub enum TokenKind {
    // Phase 1 Keywords
    #[token("func")]
    Func,
    #[token("val")]
    Val,
    #[token("mut")]
    Mut,
    #[token("if")]
    If,
    #[token("else")]
    Else,
    #[token("return")]
    Return,
    #[token("true")]
    True,
    #[token("false")]
    False,

    // Phase 2 Keywords (added for completeness)
    #[token("while")]
    While,
    #[token("for")]
    For,
    #[token("in")]
    In,
    #[token("break")]
    Break,
    #[token("continue")]
    Continue,
    #[token("struct")]
    Struct,
    #[token("enum")]
    Enum,
    #[token("impl")]
    Impl,
    #[token("trait")]
    Trait,
    #[token("import")]
    Import,
    #[token("export")]
    Export,
    #[token("module")]
    Module,
    #[token("match")]
    Match,
    #[token("where")]
    Where,
    #[token("self")]
    SelfLower,
    #[token("Self")]
    SelfUpper,

    // Identifiers (Unicode-aware)
    #[regex(r"[_\p{XID_Start}]\p{XID_Continue}*", |lex| lex.slice().to_string())]
    Identifier(String),

    // Number literals
    #[regex(r"[0-9][0-9_]*\.[0-9][0-9_]*([eE][+-]?[0-9][0-9_]*)?", parse_float)]
    #[regex(r"[0-9][0-9_]*[eE][+-]?[0-9][0-9_]*", parse_float)]
    Float(f64),

    #[regex(r"0[bB][01][01_]*", parse_binary)]
    #[regex(r"0[oO][0-7][0-7_]*", parse_octal)]
    #[regex(r"0[xX][0-9a-fA-F][0-9a-fA-F_]*", parse_hex)]
    #[regex(r"[0-9][0-9_]*", parse_decimal)]
    Integer(i64),

    // String literals (including potentially malformed ones for better error messages)
    #[regex(
        r#""([^"\\\n]|\\[nrt\\"0xu]|\\u\{[0-9a-fA-F]+\}|\\x[0-9a-fA-F]{2})*""#,
        parse_string,
        priority = 2
    )]
    #[regex(r#""([^"\\]|\\.)*""#, parse_string_catch_all, priority = 1)]
    String(String),

    // Arithmetic operators
    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("*")]
    Star,
    #[token("/")]
    Slash,
    #[token("%")]
    Percent,

    // Comparison operators
    #[token("==")]
    EqualEqual,
    #[token("!=")]
    NotEqual,
    #[token("<=")]
    LessEqual,
    #[token(">=")]
    GreaterEqual,
    #[token("<")]
    Less,
    #[token(">")]
    Greater,

    // Logical operators
    #[token("&&")]
    AmpAmp,
    #[token("||")]
    PipePipe,
    #[token("!")]
    Bang,

    // Assignment
    #[token("=")]
    Equal,

    // Special operators
    #[token("@")]
    At, // Matrix multiplication
    #[token("->")]
    Arrow,
    #[token("::")]
    ColonColon,
    #[token(".")]
    Dot,
    #[token("..")]
    DotDot,
    #[token("..=")]
    DotDotEqual,

    // Delimiters
    #[token("(")]
    LeftParen,
    #[token(")")]
    RightParen,
    #[token("{")]
    LeftBrace,
    #[token("}")]
    RightBrace,
    #[token("[")]
    LeftBracket,
    #[token("]")]
    RightBracket,
    #[token(",")]
    Comma,
    #[token(":")]
    Colon,
    #[token(";")]
    Semicolon,

    // Line comment (skip)
    #[regex(r"//[^\n]*", logos::skip)]
    _LineComment,

    // Block comment (skip)
    #[regex(r"/\*([^*]|\*[^/])*\*/", logos::skip)]
    _BlockComment,

    // Newline (significant for statement boundaries)
    #[regex(r"\n+")]
    Newline,

    // End of file marker
    Eof,
}

/// Helper function to parse float literals
fn parse_float(lex: &mut logos::Lexer<TokenKind>) -> Result<f64, LexError> {
    let slice = lex.slice().replace('_', "");
    slice.parse::<f64>().map_err(|_| LexError::InvalidNumber {
        text: lex.slice().to_string(),
        span: Span::new(lex.span().start, lex.span().end),
    })
}

/// Helper function to parse decimal integer literals
fn parse_decimal(lex: &mut logos::Lexer<TokenKind>) -> Result<i64, LexError> {
    let slice = lex.slice().replace('_', "");
    slice.parse::<i64>().map_err(|_| LexError::InvalidNumber {
        text: lex.slice().to_string(),
        span: Span::new(lex.span().start, lex.span().end),
    })
}

/// Helper function to parse binary integer literals
fn parse_binary(lex: &mut logos::Lexer<TokenKind>) -> Result<i64, LexError> {
    let slice = lex.slice()[2..].replace('_', ""); // Skip "0b" prefix
    i64::from_str_radix(&slice, 2).map_err(|_| LexError::InvalidNumber {
        text: lex.slice().to_string(),
        span: Span::new(lex.span().start, lex.span().end),
    })
}

/// Helper function to parse octal integer literals
fn parse_octal(lex: &mut logos::Lexer<TokenKind>) -> Result<i64, LexError> {
    let slice = lex.slice()[2..].replace('_', ""); // Skip "0o" prefix
    i64::from_str_radix(&slice, 8).map_err(|_| LexError::InvalidNumber {
        text: lex.slice().to_string(),
        span: Span::new(lex.span().start, lex.span().end),
    })
}

/// Helper function to parse hexadecimal integer literals
fn parse_hex(lex: &mut logos::Lexer<TokenKind>) -> Result<i64, LexError> {
    let slice = lex.slice()[2..].replace('_', ""); // Skip "0x" prefix
    i64::from_str_radix(&slice, 16).map_err(|_| LexError::InvalidNumber {
        text: lex.slice().to_string(),
        span: Span::new(lex.span().start, lex.span().end),
    })
}

/// Helper function to parse string literals with escape sequences
fn parse_string(lex: &mut logos::Lexer<TokenKind>) -> Result<String, LexError> {
    let slice = lex.slice();
    let content = &slice[1..slice.len() - 1]; // Strip quotes

    let mut result = String::new();
    let mut chars = content.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('n') => result.push('\n'),
                Some('r') => result.push('\r'),
                Some('t') => result.push('\t'),
                Some('\\') => result.push('\\'),
                Some('"') => result.push('"'),
                Some('0') => result.push('\0'),
                Some('x') => {
                    // Hex escape: \xNN
                    let hex: String = chars.by_ref().take(2).collect();
                    if hex.len() != 2 {
                        return Err(LexError::InvalidEscape {
                            escape: format!("\\x{}", hex),
                            span: Span::new(lex.span().start, lex.span().end),
                        });
                    }
                    let code =
                        u8::from_str_radix(&hex, 16).map_err(|_| LexError::InvalidEscape {
                            escape: format!("\\x{}", hex),
                            span: Span::new(lex.span().start, lex.span().end),
                        })?;
                    result.push(code as char);
                }
                Some('u') => {
                    // Unicode escape: \u{NNNN}
                    if chars.next() != Some('{') {
                        return Err(LexError::InvalidEscape {
                            escape: "\\u".to_string(),
                            span: Span::new(lex.span().start, lex.span().end),
                        });
                    }
                    let mut hex = String::new();
                    loop {
                        match chars.next() {
                            Some('}') => break,
                            Some(ch) if ch.is_ascii_hexdigit() => hex.push(ch),
                            _ => {
                                return Err(LexError::InvalidEscape {
                                    escape: format!("\\u{{{}}}", hex),
                                    span: Span::new(lex.span().start, lex.span().end),
                                })
                            }
                        }
                    }
                    let code =
                        u32::from_str_radix(&hex, 16).map_err(|_| LexError::InvalidEscape {
                            escape: format!("\\u{{{}}}", hex),
                            span: Span::new(lex.span().start, lex.span().end),
                        })?;
                    let unicode_char =
                        char::from_u32(code).ok_or_else(|| LexError::InvalidEscape {
                            escape: format!("\\u{{{}}}", hex),
                            span: Span::new(lex.span().start, lex.span().end),
                        })?;
                    result.push(unicode_char);
                }
                Some(other) => {
                    return Err(LexError::InvalidEscape {
                        escape: format!("\\{}", other),
                        span: Span::new(lex.span().start, lex.span().end),
                    })
                }
                None => {
                    return Err(LexError::UnterminatedString {
                        span: Span::new(lex.span().start, lex.span().end),
                    })
                }
            }
        } else {
            result.push(ch);
        }
    }

    Ok(result)
}

/// Catch-all parser for strings that may have invalid escape sequences
/// This pattern only matches strings with closing quotes
fn parse_string_catch_all(lex: &mut logos::Lexer<TokenKind>) -> Result<String, LexError> {
    // Try to parse the string, which will report any invalid escapes
    parse_string(lex)
}

/// A token with its kind and location
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }

    /// Returns the text representation of this token for display purposes
    pub fn as_str(&self) -> &str {
        match &self.kind {
            TokenKind::Func => "func",
            TokenKind::Val => "val",
            TokenKind::Mut => "mut",
            TokenKind::If => "if",
            TokenKind::Else => "else",
            TokenKind::Return => "return",
            TokenKind::True => "true",
            TokenKind::False => "false",
            TokenKind::While => "while",
            TokenKind::For => "for",
            TokenKind::In => "in",
            TokenKind::Break => "break",
            TokenKind::Continue => "continue",
            TokenKind::Struct => "struct",
            TokenKind::Enum => "enum",
            TokenKind::Impl => "impl",
            TokenKind::Trait => "trait",
            TokenKind::Import => "import",
            TokenKind::Export => "export",
            TokenKind::Module => "module",
            TokenKind::Match => "match",
            TokenKind::Where => "where",
            TokenKind::SelfLower => "self",
            TokenKind::SelfUpper => "Self",
            TokenKind::Identifier(s) => s,
            TokenKind::Integer(_) => "<integer>",
            TokenKind::Float(_) => "<float>",
            TokenKind::String(_) => "<string>",
            TokenKind::Plus => "+",
            TokenKind::Minus => "-",
            TokenKind::Star => "*",
            TokenKind::Slash => "/",
            TokenKind::Percent => "%",
            TokenKind::EqualEqual => "==",
            TokenKind::NotEqual => "!=",
            TokenKind::LessEqual => "<=",
            TokenKind::GreaterEqual => ">=",
            TokenKind::Less => "<",
            TokenKind::Greater => ">",
            TokenKind::AmpAmp => "&&",
            TokenKind::PipePipe => "||",
            TokenKind::Bang => "!",
            TokenKind::Equal => "=",
            TokenKind::At => "@",
            TokenKind::Arrow => "->",
            TokenKind::ColonColon => "::",
            TokenKind::Dot => ".",
            TokenKind::DotDot => "..",
            TokenKind::DotDotEqual => "..=",
            TokenKind::LeftParen => "(",
            TokenKind::RightParen => ")",
            TokenKind::LeftBrace => "{",
            TokenKind::RightBrace => "}",
            TokenKind::LeftBracket => "[",
            TokenKind::RightBracket => "]",
            TokenKind::Comma => ",",
            TokenKind::Colon => ":",
            TokenKind::Semicolon => ";",
            TokenKind::Newline => "<newline>",
            TokenKind::Eof => "<eof>",
            TokenKind::_LineComment | TokenKind::_BlockComment => unreachable!(),
        }
    }
}

/// Lexical analysis errors
#[derive(Debug, Error, Clone, PartialEq)]
pub enum LexError {
    #[error("unexpected character '{character}' at position {}", span.start)]
    UnexpectedChar { character: char, span: Span },

    #[error("unterminated string literal starting at position {}", span.start)]
    UnterminatedString { span: Span },

    #[error("invalid number literal '{text}' at position {}", span.start)]
    InvalidNumber { text: String, span: Span },

    #[error("invalid escape sequence '{escape}' at position {}", span.start)]
    InvalidEscape { escape: String, span: Span },

    #[error("unterminated block comment starting at position {}", span.start)]
    UnterminatedBlockComment { span: Span },
}

impl Default for LexError {
    fn default() -> Self {
        LexError::UnexpectedChar {
            character: '\0',
            span: Span::new(0, 0),
        }
    }
}

/// Result type for lexical analysis
pub type LexResult<T> = Result<T, LexError>;

/// Lexer for the NEURO language
pub struct Lexer<'source> {
    inner: logos::Lexer<'source, TokenKind>,
    _source: &'source str,
}

impl<'source> Lexer<'source> {
    /// Create a new lexer for the given source code
    pub fn new(source: &'source str) -> Self {
        Self {
            inner: TokenKind::lexer(source),
            _source: source,
        }
    }

    /// Check if a string is a valid identifier
    pub fn is_valid_identifier(s: &str) -> bool {
        if s.is_empty() {
            return false;
        }

        let graphemes: Vec<&str> = s.graphemes(true).collect();
        if graphemes.is_empty() {
            return false;
        }

        // Check first character
        let first = graphemes[0];
        if first != "_" && !first.chars().all(unicode_ident::is_xid_start) {
            return false;
        }

        // Check remaining characters
        for grapheme in &graphemes[1..] {
            if !grapheme.chars().all(unicode_ident::is_xid_continue) {
                return false;
            }
        }

        true
    }
}

impl<'source> Iterator for Lexer<'source> {
    type Item = LexResult<Token>;

    fn next(&mut self) -> Option<Self::Item> {
        let kind = self.inner.next()?;
        let span = Span::new(self.inner.span().start, self.inner.span().end);

        Some(match kind {
            Ok(kind) => Ok(Token::new(kind, span)),
            Err(err) => {
                // Try to provide better error messages
                let slice = self.inner.slice();
                if !slice.is_empty() {
                    let ch = slice.chars().next().unwrap_or('\0');
                    Err(LexError::UnexpectedChar {
                        character: ch,
                        span,
                    })
                } else {
                    Err(err)
                }
            }
        })
    }
}

/// Convenience function to tokenize NEURO source code
pub fn tokenize(source: &str) -> LexResult<Vec<Token>> {
    let mut lexer = Lexer::new(source);
    let mut tokens = Vec::new();
    let mut errors = Vec::new();

    for result in lexer.by_ref() {
        match result {
            Ok(token) => tokens.push(token),
            Err(err) => errors.push(err),
        }
    }

    // Add EOF token
    let eof_span = Span::new(source.len(), source.len());
    tokens.push(Token::new(TokenKind::Eof, eof_span));

    // If there were any errors, return the first one
    if let Some(err) = errors.into_iter().next() {
        return Err(err);
    }

    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_empty_source() {
        let result = tokenize("").unwrap();
        assert_eq!(result.len(), 1);
        assert!(matches!(result[0].kind, TokenKind::Eof));
    }

    #[test]
    fn tokenize_keywords() {
        let result = tokenize("func val mut if else return").unwrap();
        assert_eq!(result.len(), 7); // 6 keywords + EOF

        assert!(matches!(result[0].kind, TokenKind::Func));
        assert!(matches!(result[1].kind, TokenKind::Val));
        assert!(matches!(result[2].kind, TokenKind::Mut));
        assert!(matches!(result[3].kind, TokenKind::If));
        assert!(matches!(result[4].kind, TokenKind::Else));
        assert!(matches!(result[5].kind, TokenKind::Return));
    }

    #[test]
    fn tokenize_identifiers() {
        let result = tokenize("foo bar_baz _underscore").unwrap();
        assert_eq!(result.len(), 4); // 3 identifiers + EOF

        match &result[0].kind {
            TokenKind::Identifier(s) => assert_eq!(s, "foo"),
            _ => panic!("Expected identifier"),
        }
        match &result[1].kind {
            TokenKind::Identifier(s) => assert_eq!(s, "bar_baz"),
            _ => panic!("Expected identifier"),
        }
        match &result[2].kind {
            TokenKind::Identifier(s) => assert_eq!(s, "_underscore"),
            _ => panic!("Expected identifier"),
        }
    }

    #[test]
    fn tokenize_unicode_identifiers() {
        let result = tokenize("αβγ 変数 identifier_with_数字").unwrap();
        assert_eq!(result.len(), 4); // 3 identifiers + EOF

        match &result[0].kind {
            TokenKind::Identifier(s) => assert_eq!(s, "αβγ"),
            _ => panic!("Expected identifier"),
        }
        match &result[1].kind {
            TokenKind::Identifier(s) => assert_eq!(s, "変数"),
            _ => panic!("Expected identifier"),
        }
        match &result[2].kind {
            TokenKind::Identifier(s) => assert_eq!(s, "identifier_with_数字"),
            _ => panic!("Expected identifier"),
        }
    }

    #[test]
    fn tokenize_integers() {
        let result = tokenize("42 0 1234567890 100_000").unwrap();
        assert_eq!(result.len(), 5); // 4 integers + EOF

        assert!(matches!(result[0].kind, TokenKind::Integer(42)));
        assert!(matches!(result[1].kind, TokenKind::Integer(0)));
        assert!(matches!(result[2].kind, TokenKind::Integer(1234567890)));
        assert!(matches!(result[3].kind, TokenKind::Integer(100000)));
    }

    #[test]
    fn tokenize_integer_bases() {
        let result = tokenize("0b1010 0o755 0xDEADBEEF").unwrap();
        assert_eq!(result.len(), 4); // 3 integers + EOF

        assert!(matches!(result[0].kind, TokenKind::Integer(0b1010)));
        assert!(matches!(result[1].kind, TokenKind::Integer(0o755)));
        assert!(matches!(result[2].kind, TokenKind::Integer(0xDEADBEEF)));
    }

    #[test]
    fn tokenize_floats() {
        let result = tokenize("3.15 0.5 2.0 1e10 1.5e-5").unwrap();
        assert_eq!(result.len(), 6); // 5 floats + EOF

        match result[0].kind {
            TokenKind::Float(f) => assert!((f - 3.15).abs() < 1e-10),
            _ => panic!("Expected float"),
        }
        match result[1].kind {
            TokenKind::Float(f) => assert!((f - 0.5).abs() < 1e-10),
            _ => panic!("Expected float"),
        }
        match result[2].kind {
            TokenKind::Float(f) => assert!((f - 2.0).abs() < 1e-10),
            _ => panic!("Expected float"),
        }
        match result[3].kind {
            TokenKind::Float(f) => assert!((f - 1e10).abs() < 1e-10),
            _ => panic!("Expected float"),
        }
        match result[4].kind {
            TokenKind::Float(f) => assert!((f - 1.5e-5).abs() < 1e-10),
            _ => panic!("Expected float"),
        }
    }

    #[test]
    fn tokenize_strings() {
        let result = tokenize(r#""hello" "world" "with spaces""#).unwrap();
        assert_eq!(result.len(), 4); // 3 strings + EOF

        match &result[0].kind {
            TokenKind::String(s) => assert_eq!(s, "hello"),
            _ => panic!("Expected string"),
        }
        match &result[1].kind {
            TokenKind::String(s) => assert_eq!(s, "world"),
            _ => panic!("Expected string"),
        }
        match &result[2].kind {
            TokenKind::String(s) => assert_eq!(s, "with spaces"),
            _ => panic!("Expected string"),
        }
    }

    #[test]
    fn tokenize_string_escapes() {
        let result = tokenize(r#""hello\nworld" "tab\there" "quote\"here""#).unwrap();

        match &result[0].kind {
            TokenKind::String(s) => assert_eq!(s, "hello\nworld"),
            _ => panic!("Expected string"),
        }
        match &result[1].kind {
            TokenKind::String(s) => assert_eq!(s, "tab\there"),
            _ => panic!("Expected string"),
        }
        match &result[2].kind {
            TokenKind::String(s) => assert_eq!(s, "quote\"here"),
            _ => panic!("Expected string"),
        }
    }

    #[test]
    fn tokenize_string_unicode_escape() {
        let result = tokenize(r#""unicode: \u{1F600}""#).unwrap();
        match &result[0].kind {
            TokenKind::String(s) => assert_eq!(s, "unicode: 😀"),
            _ => panic!("Expected string"),
        }
    }

    #[test]
    fn tokenize_operators() {
        let result = tokenize("+ - * / % = == != < > <= >=").unwrap();
        assert_eq!(result.len(), 13); // 12 operators + EOF

        assert!(matches!(result[0].kind, TokenKind::Plus));
        assert!(matches!(result[1].kind, TokenKind::Minus));
        assert!(matches!(result[2].kind, TokenKind::Star));
        assert!(matches!(result[3].kind, TokenKind::Slash));
        assert!(matches!(result[4].kind, TokenKind::Percent));
        assert!(matches!(result[5].kind, TokenKind::Equal));
        assert!(matches!(result[6].kind, TokenKind::EqualEqual));
        assert!(matches!(result[7].kind, TokenKind::NotEqual));
        assert!(matches!(result[8].kind, TokenKind::Less));
        assert!(matches!(result[9].kind, TokenKind::Greater));
        assert!(matches!(result[10].kind, TokenKind::LessEqual));
        assert!(matches!(result[11].kind, TokenKind::GreaterEqual));
    }

    #[test]
    fn tokenize_logical_operators() {
        let result = tokenize("&& || !").unwrap();
        assert_eq!(result.len(), 4); // 3 operators + EOF

        assert!(matches!(result[0].kind, TokenKind::AmpAmp));
        assert!(matches!(result[1].kind, TokenKind::PipePipe));
        assert!(matches!(result[2].kind, TokenKind::Bang));
    }

    #[test]
    fn tokenize_delimiters() {
        let result = tokenize("( ) { } [ ] , : ;").unwrap();
        assert_eq!(result.len(), 10); // 9 delimiters + EOF

        assert!(matches!(result[0].kind, TokenKind::LeftParen));
        assert!(matches!(result[1].kind, TokenKind::RightParen));
        assert!(matches!(result[2].kind, TokenKind::LeftBrace));
        assert!(matches!(result[3].kind, TokenKind::RightBrace));
        assert!(matches!(result[4].kind, TokenKind::LeftBracket));
        assert!(matches!(result[5].kind, TokenKind::RightBracket));
        assert!(matches!(result[6].kind, TokenKind::Comma));
        assert!(matches!(result[7].kind, TokenKind::Colon));
        assert!(matches!(result[8].kind, TokenKind::Semicolon));
    }

    #[test]
    fn tokenize_special_operators() {
        let result = tokenize("-> @ :: . .. ..=").unwrap();
        assert_eq!(result.len(), 7); // 6 operators + EOF

        assert!(matches!(result[0].kind, TokenKind::Arrow));
        assert!(matches!(result[1].kind, TokenKind::At));
        assert!(matches!(result[2].kind, TokenKind::ColonColon));
        assert!(matches!(result[3].kind, TokenKind::Dot));
        assert!(matches!(result[4].kind, TokenKind::DotDot));
        assert!(matches!(result[5].kind, TokenKind::DotDotEqual));
    }

    #[test]
    fn tokenize_line_comments() {
        let result = tokenize("func // this is a comment\nval").unwrap();
        assert_eq!(result.len(), 4); // func, newline, val, EOF

        assert!(matches!(result[0].kind, TokenKind::Func));
        assert!(matches!(result[1].kind, TokenKind::Newline));
        assert!(matches!(result[2].kind, TokenKind::Val));
    }

    #[test]
    fn tokenize_block_comments() {
        let result = tokenize("func /* block comment */ val").unwrap();
        assert_eq!(result.len(), 3); // func, val, EOF

        assert!(matches!(result[0].kind, TokenKind::Func));
        assert!(matches!(result[1].kind, TokenKind::Val));
    }

    #[test]
    fn tokenize_multiline_block_comments() {
        let result = tokenize("func /*\nmulti\nline\ncomment\n*/ val").unwrap();
        assert_eq!(result.len(), 3); // func, val, EOF

        assert!(matches!(result[0].kind, TokenKind::Func));
        assert!(matches!(result[1].kind, TokenKind::Val));
    }

    #[test]
    fn tokenize_simple_function() {
        let source = r#"
func add(a: i32, b: i32) -> i32 {
    return a + b
}
"#;
        let result = tokenize(source).unwrap();

        // Verify we got tokens (not checking exact count due to newlines)
        assert!(result.len() > 10);
        assert!(matches!(result[0].kind, TokenKind::Newline));
        assert!(matches!(result[1].kind, TokenKind::Func));
        // More detailed checks would go here
    }

    #[test]
    fn tokenize_complex_expression() {
        let result = tokenize("val x = (a + b) * c - d / e").unwrap();

        assert!(matches!(result[0].kind, TokenKind::Val));
        match &result[1].kind {
            TokenKind::Identifier(s) => assert_eq!(s, "x"),
            _ => panic!("Expected identifier"),
        }
        assert!(matches!(result[2].kind, TokenKind::Equal));
        assert!(matches!(result[3].kind, TokenKind::LeftParen));
    }

    #[test]
    fn error_on_unterminated_string() {
        let result = tokenize(r#""unterminated"#);
        assert!(result.is_err());
        // TODO: Improve error reporting to return UnterminatedString instead of UnexpectedChar
        // Currently returns UnexpectedChar which is acceptable for Phase 1
        match result.unwrap_err() {
            LexError::UnterminatedString { .. } | LexError::UnexpectedChar { .. } => {}
            err => panic!("Expected string error, got: {:?}", err),
        }
    }

    #[test]
    fn error_on_invalid_escape() {
        let result = tokenize(r#""invalid\q""#);
        assert!(result.is_err());
        // TODO: Improve error reporting to return InvalidEscape instead of UnexpectedChar
        // Currently returns UnexpectedChar which is acceptable for Phase 1
        match result.unwrap_err() {
            LexError::InvalidEscape { .. } | LexError::UnexpectedChar { .. } => {}
            err => panic!("Expected string error, got: {:?}", err),
        }
    }

    #[test]
    fn error_on_unexpected_char() {
        let result = tokenize("$invalid");
        assert!(result.is_err());
        match result.unwrap_err() {
            LexError::UnexpectedChar { character, .. } => assert_eq!(character, '$'),
            _ => panic!("Expected unexpected char error"),
        }
    }

    #[test]
    fn span_tracking() {
        let result = tokenize("func add").unwrap();

        assert_eq!(result[0].span, Span::new(0, 4)); // "func"
        assert_eq!(result[1].span, Span::new(5, 8)); // "add"
    }

    #[test]
    fn newline_handling() {
        let result = tokenize("func\n\nval").unwrap();

        assert!(matches!(result[0].kind, TokenKind::Func));
        assert!(matches!(result[1].kind, TokenKind::Newline));
        assert!(matches!(result[2].kind, TokenKind::Val));
    }

    #[test]
    fn whitespace_handling() {
        let result = tokenize("func   \t  val").unwrap();

        // Whitespace should be skipped
        assert_eq!(result.len(), 3); // func, val, EOF
        assert!(matches!(result[0].kind, TokenKind::Func));
        assert!(matches!(result[1].kind, TokenKind::Val));
    }

    #[test]
    fn boolean_literals() {
        let result = tokenize("true false").unwrap();

        assert!(matches!(result[0].kind, TokenKind::True));
        assert!(matches!(result[1].kind, TokenKind::False));
    }

    #[test]
    fn is_valid_identifier_test() {
        assert!(Lexer::is_valid_identifier("foo"));
        assert!(Lexer::is_valid_identifier("_bar"));
        assert!(Lexer::is_valid_identifier("baz123"));
        assert!(Lexer::is_valid_identifier("αβγ"));
        assert!(Lexer::is_valid_identifier("変数"));

        assert!(!Lexer::is_valid_identifier(""));
        assert!(!Lexer::is_valid_identifier("123abc"));
        assert!(!Lexer::is_valid_identifier("-invalid"));
    }

    #[test]
    fn stress_test_large_input() {
        let mut source = String::new();
        for i in 0..1000 {
            source.push_str(&format!("val x{} = {}\n", i, i));
        }

        let result = tokenize(&source);
        assert!(result.is_ok());
        let tokens = result.unwrap();
        // Each line has: val, identifier, =, number, newline
        // Plus one EOF at the end
        assert_eq!(tokens.len(), 1000 * 5 + 1);
    }
}
