// Token type definitions

use logos::Logos;
use shared_types::{FloatSuffix, IntSuffix, Span};

use crate::errors::LexError;

/// Carries both the numeric value and the explicit type suffix of a suffixed
/// integer literal (e.g. `42i64`, `255u8`).
#[derive(Debug, Clone, PartialEq)]
pub struct IntegerSuffixToken {
    pub value: i64,
    pub suffix: IntSuffix,
}

/// Carries both the numeric value and the explicit type suffix of a suffixed
/// float literal (e.g. `1.5f32`, `2.0f64`, `1e10f32`).
#[derive(Debug, Clone, PartialEq)]
pub struct FloatSuffixToken {
    pub value: f64,
    pub suffix: FloatSuffix,
}

/// Token types in the Neuro language
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
    #[token("const")]
    Const,
    #[token("as")]
    As,
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
    #[token("loop")]
    Loop,
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
    #[token("type")]
    Type,
    #[token("newtype")]
    Newtype,
    #[token("unsafe")]
    Unsafe,
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

    // Suffixed float literals. Priority above the bare-Float patterns so logos
    // longest-match picks `1.5f32` as a single FloatSuffix token rather than
    // Float(1.5) + Identifier("f32"). Two patterns mirror the fractional and
    // exponent-only forms of the Float regex. `f16`/`bf16` are the half-precision
    // suffixes (§1.2); `bf16` precedes the others in the alternation only for
    // readability — logos matches the whole literal greedily regardless.
    #[regex(
        r"[0-9][0-9_]*\.[0-9][0-9_]*([eE][+-]?[0-9][0-9_]*)?(bf16|f16|f32|f64)",
        parse_fractional_float_suffix,
        priority = 3
    )]
    #[regex(
        r"[0-9][0-9_]*[eE][+-]?[0-9][0-9_]*(bf16|f16|f32|f64)",
        parse_fractional_float_suffix,
        priority = 3
    )]
    FloatSuffix(FloatSuffixToken),

    // Suffixed integer literals (higher priority than plain; logos maximal munch picks the longer
    // match for `42i64` → IntegerSuffix rather than Integer(42) + Identifier("i64")).
    #[regex(
        r"[0-9][0-9_]*(i8|i16|i32|i64|u8|u16|u32|u64)",
        parse_decimal_suffix,
        priority = 2
    )]
    #[regex(
        r"0[bB][01][01_]*(i8|i16|i32|i64|u8|u16|u32|u64)",
        parse_binary_suffix,
        priority = 2
    )]
    #[regex(
        r"0[oO][0-7][0-7_]*(i8|i16|i32|i64|u8|u16|u32|u64)",
        parse_octal_suffix,
        priority = 2
    )]
    #[regex(
        r"0[xX][0-9a-fA-F][0-9a-fA-F_]*(i8|i16|i32|i64|u8|u16|u32|u64)",
        parse_hex_suffix,
        priority = 2
    )]
    IntegerSuffix(IntegerSuffixToken),

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

    // Character literals (§1.2): a single Unicode scalar value between single
    // quotes, e.g. `'a'`, `'\n'`, `'\u{1F44D}'`. The regex admits exactly one
    // content unit — a non-quote/backslash/newline char, a recognized escape, a
    // `\u{...}` unicode escape, or a `\xNN` byte escape — so `''`, `'ab'`, and an
    // unterminated `'a` never match and fall through to a lex error.
    #[regex(
        r"'([^'\\\n]|\\['nrt\\0]|\\u\{[0-9a-fA-F]+\}|\\x[0-9a-fA-F]{2})'",
        parse_char
    )]
    Char(char),

    // Lifetime name (§2.6): a leading `'` followed by an identifier, with NO closing
    // quote — e.g. `'a` in `func longest<'a>(...)`. The callback strips the `'`, so the
    // stored name is the bare identifier. A char literal `'a'` is a strictly longer match
    // (it carries the closing quote), so logos' longest-match rule keeps char literals
    // winning; only the quote-less form reaches here.
    #[regex(r"'[_\p{XID_Start}]\p{XID_Continue}*", |lex| lex.slice()[1..].to_string())]
    Lifetime(String),

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

    // Compound assignment operators (must appear before single-char arithmetic tokens
    // in the logos dispatch table; longest-match ensures += beats + then =)
    #[token("+=")]
    PlusEqual,
    #[token("-=")]
    MinusEqual,
    #[token("*=")]
    StarEqual,
    #[token("/=")]
    SlashEqual,
    #[token("%=")]
    PercentEqual,

    // Comparison operators (two-character ops must come before single-character)
    #[token("==")]
    EqualEqual,
    #[token("!=")]
    NotEqual,
    #[token("<=")]
    LessEqual,
    #[token(">=")]
    GreaterEqual,
    // LeftShift must precede Less so logos longest-match picks `<<` over `<`
    #[token("<<")]
    LeftShift,
    #[token("<")]
    Less,
    #[token(">")]
    Greater,

    // Logical and bitwise operators
    #[token("&&")]
    AmpAmp,
    #[token("&")]
    Amp,
    #[token("||")]
    PipePipe,
    #[token("|")]
    Pipe,
    #[token("^")]
    Caret,
    #[token("~")]
    Tilde,
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
    #[token("=>")]
    FatArrow,
    #[token("::")]
    ColonColon,
    #[token("..=")]
    DotDotEqual,
    #[token("..")]
    DotDot,
    #[token(".")]
    Dot,
    // Null/error coalescing. Full semantics arrive in Phase 2 with Option/Result;
    // tokenized + parsed now so the R-to-L precedence (Appendix B row 14) is locked in.
    #[token("??")]
    QuestionQuestion,

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
            TokenKind::Const => "const",
            TokenKind::As => "as",
            TokenKind::If => "if",
            TokenKind::Else => "else",
            TokenKind::Return => "return",
            TokenKind::True => "true",
            TokenKind::False => "false",
            TokenKind::While => "while",
            TokenKind::Loop => "loop",
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
            TokenKind::Type => "type",
            TokenKind::Newtype => "newtype",
            TokenKind::Unsafe => "unsafe",
            TokenKind::SelfLower => "self",
            TokenKind::SelfUpper => "Self",
            TokenKind::Identifier(s) => s,
            TokenKind::Integer(_) => "<integer>",
            TokenKind::IntegerSuffix(_) => "<integer>",
            TokenKind::Float(_) => "<float>",
            TokenKind::FloatSuffix(_) => "<float>",
            TokenKind::String(_) => "<string>",
            TokenKind::Char(_) => "<char>",
            TokenKind::Lifetime(_) => "<lifetime>",
            TokenKind::Plus => "+",
            TokenKind::Minus => "-",
            TokenKind::Star => "*",
            TokenKind::Slash => "/",
            TokenKind::Percent => "%",
            TokenKind::PlusEqual => "+=",
            TokenKind::MinusEqual => "-=",
            TokenKind::StarEqual => "*=",
            TokenKind::SlashEqual => "/=",
            TokenKind::PercentEqual => "%=",
            TokenKind::EqualEqual => "==",
            TokenKind::NotEqual => "!=",
            TokenKind::LessEqual => "<=",
            TokenKind::GreaterEqual => ">=",
            TokenKind::Less => "<",
            TokenKind::Greater => ">",
            TokenKind::LeftShift => "<<",
            TokenKind::AmpAmp => "&&",
            TokenKind::Amp => "&",
            TokenKind::PipePipe => "||",
            TokenKind::Pipe => "|",
            TokenKind::Caret => "^",
            TokenKind::Tilde => "~",
            TokenKind::Bang => "!",
            TokenKind::Equal => "=",
            TokenKind::At => "@",
            TokenKind::Arrow => "->",
            TokenKind::FatArrow => "=>",
            TokenKind::ColonColon => "::",
            TokenKind::Dot => ".",
            TokenKind::DotDot => "..",
            TokenKind::DotDotEqual => "..=",
            TokenKind::QuestionQuestion => "??",
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

/// Parse a character literal into its single Unicode scalar value (§1.2). The
/// regex guarantees exactly one content unit between the quotes; this decodes a
/// recognized escape (`\n`, `\u{...}`, `\xNN`, …) or returns the lone character.
/// A `\u{...}` payload outside the valid scalar range (e.g. a surrogate) is the
/// one case the regex cannot reject, so it is validated here.
fn parse_char(lex: &mut logos::Lexer<TokenKind>) -> Result<char, LexError> {
    let slice = lex.slice();
    let span = Span::new(lex.span().start, lex.span().end);
    let content = &slice[1..slice.len() - 1]; // Strip the surrounding single quotes
    let mut chars = content.chars();

    let invalid = |inner: &str| LexError::InvalidCharLiteral {
        literal: format!("'{}'", inner),
        span,
    };

    let first = chars.next().ok_or_else(|| invalid(content))?;
    if first != '\\' {
        return Ok(first);
    }

    match chars.next() {
        Some('n') => Ok('\n'),
        Some('r') => Ok('\r'),
        Some('t') => Ok('\t'),
        Some('\\') => Ok('\\'),
        Some('\'') => Ok('\''),
        Some('0') => Ok('\0'),
        Some('x') => {
            let hex: String = chars.by_ref().take(2).collect();
            let code = u8::from_str_radix(&hex, 16).map_err(|_| invalid(content))?;
            Ok(code as char)
        }
        Some('u') => {
            // `\u{NNNN}` — the regex shape is fixed, so skip the leading `{` and
            // read hex digits until `}`.
            let hex: String = chars.take_while(|&c| c != '}').skip(1).collect();
            let code = u32::from_str_radix(&hex, 16).map_err(|_| invalid(content))?;
            char::from_u32(code).ok_or_else(|| invalid(content))
        }
        _ => Err(invalid(content)),
    }
}

// ── Suffixed integer helpers ──────────────────────────────────────────────────

/// Maps the suffix string (e.g. "i64") to `IntSuffix`. Panics for unexpected
/// inputs — the logos regex guarantees the suffix is one of the eight variants.
fn parse_int_suffix(suffix: &str) -> IntSuffix {
    match suffix {
        "i8" => IntSuffix::I8,
        "i16" => IntSuffix::I16,
        "i32" => IntSuffix::I32,
        "i64" => IntSuffix::I64,
        "u8" => IntSuffix::U8,
        "u16" => IntSuffix::U16,
        "u32" => IntSuffix::U32,
        "u64" => IntSuffix::U64,
        // Safety: the regex only admits the eight suffixes above.
        _ => unreachable!("unexpected suffix '{}'", suffix),
    }
}

fn parse_decimal_suffix(lex: &mut logos::Lexer<TokenKind>) -> Result<IntegerSuffixToken, LexError> {
    let raw = lex.slice();
    let suffix_start = raw.find(|c: char| c.is_alphabetic()).unwrap_or(raw.len());
    let digits = raw[..suffix_start].replace('_', "");
    let value = digits.parse::<i64>().map_err(|_| LexError::InvalidNumber {
        text: raw.to_string(),
        span: Span::new(lex.span().start, lex.span().end),
    })?;
    Ok(IntegerSuffixToken {
        value,
        suffix: parse_int_suffix(&raw[suffix_start..]),
    })
}

fn parse_binary_suffix(lex: &mut logos::Lexer<TokenKind>) -> Result<IntegerSuffixToken, LexError> {
    let raw = lex.slice();
    let suffix_start = raw[2..]
        .find(|c: char| c.is_alphabetic())
        .map(|i| i + 2)
        .unwrap_or(raw.len());
    let digits = raw[2..suffix_start].replace('_', "");
    let value = i64::from_str_radix(&digits, 2).map_err(|_| LexError::InvalidNumber {
        text: raw.to_string(),
        span: Span::new(lex.span().start, lex.span().end),
    })?;
    Ok(IntegerSuffixToken {
        value,
        suffix: parse_int_suffix(&raw[suffix_start..]),
    })
}

fn parse_octal_suffix(lex: &mut logos::Lexer<TokenKind>) -> Result<IntegerSuffixToken, LexError> {
    let raw = lex.slice();
    let suffix_start = raw[2..]
        .find(|c: char| c.is_alphabetic())
        .map(|i| i + 2)
        .unwrap_or(raw.len());
    let digits = raw[2..suffix_start].replace('_', "");
    let value = i64::from_str_radix(&digits, 8).map_err(|_| LexError::InvalidNumber {
        text: raw.to_string(),
        span: Span::new(lex.span().start, lex.span().end),
    })?;
    Ok(IntegerSuffixToken {
        value,
        suffix: parse_int_suffix(&raw[suffix_start..]),
    })
}

// ── Suffixed float helpers ────────────────────────────────────────────────────

/// Splits a suffixed float literal into its digit portion and `FloatSuffix`.
///
/// `bf16` is checked before `f16` because `"...bf16"` also ends in `"f16"`;
/// stripping the shorter suffix first would leave a stray `b` in the digits.
fn split_float_suffix(raw: &str) -> Option<(&str, FloatSuffix)> {
    const SUFFIXES: [(&str, FloatSuffix); 4] = [
        ("bf16", FloatSuffix::BF16),
        ("f16", FloatSuffix::F16),
        ("f32", FloatSuffix::F32),
        ("f64", FloatSuffix::F64),
    ];
    SUFFIXES
        .iter()
        .find_map(|(s, suffix)| raw.strip_suffix(s).map(|digits| (digits, *suffix)))
}

/// Parses a float-suffix literal in either fractional (`1.5f32`) or
/// exponent-only (`1e10f32`) form. The trailing suffix (`f16`/`bf16`/`f32`/`f64`)
/// is split off; the digit portion is parsed by Rust's `f64` parser after
/// stripping underscore separators.
fn parse_fractional_float_suffix(
    lex: &mut logos::Lexer<TokenKind>,
) -> Result<FloatSuffixToken, LexError> {
    let raw = lex.slice();
    let invalid = || LexError::InvalidNumber {
        text: raw.to_string(),
        span: Span::new(lex.span().start, lex.span().end),
    };
    // Safety: the regex only admits the four recognized suffixes.
    let (digits, suffix) = split_float_suffix(raw).ok_or_else(invalid)?;
    let value = digits
        .replace('_', "")
        .parse::<f64>()
        .map_err(|_| invalid())?;
    Ok(FloatSuffixToken { value, suffix })
}

fn parse_hex_suffix(lex: &mut logos::Lexer<TokenKind>) -> Result<IntegerSuffixToken, LexError> {
    let raw = lex.slice();
    // Skip "0x" prefix; find first alphabetic that is NOT a hex digit (a-f/A-F)
    let after_prefix = &raw[2..];
    let suffix_start = after_prefix
        .find(|c: char| c.is_alphabetic() && !matches!(c, 'a'..='f' | 'A'..='F'))
        .map(|i| i + 2)
        .unwrap_or(raw.len());
    let digits = raw[2..suffix_start].replace('_', "");
    let value = i64::from_str_radix(&digits, 16).map_err(|_| LexError::InvalidNumber {
        text: raw.to_string(),
        span: Span::new(lex.span().start, lex.span().end),
    })?;
    Ok(IntegerSuffixToken {
        value,
        suffix: parse_int_suffix(&raw[suffix_start..]),
    })
}
