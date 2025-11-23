// NEURO Programming Language - Lexical Analysis
// Token type definitions

use logos::Logos;
use shared_types::Span;

use crate::errors::LexError;

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

    // Comparison operators (two-character ops must come before single-character)
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
    At,
    #[token("->")]
    Arrow,
    #[token("::")]
    ColonColon,
    #[token("..=")]
    DotDotEqual,
    #[token("..")]
    DotDot,
    #[token(".")]
    Dot,

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

    // Comments and whitespace
    #[regex(r"//[^\n]*", logos::skip)]
    _LineComment,
    #[regex(r"/\*([^*]|\*[^/])*\*/", logos::skip)]
    _BlockComment,
    #[regex(r"\n+")]
    Newline,

    // End of file
    Eof,
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

// Literal parsing helper functions (tightly coupled to TokenKind)

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
