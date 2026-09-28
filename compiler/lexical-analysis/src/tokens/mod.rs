// Token type definitions
//
// The editor's TextMate grammar (`neuro-language-support/syntaxes/neuro.tmLanguage.json`)
// re-describes these tokens as regexes and has no build-time link to this file.
// `tests/tmlanguage_sync.rs` scans the `#[token("…")]` literals below and fails when a
// keyword is missing there; rules with no one-to-one token (string bodies, escapes) are
// not covered and must be updated by hand.

use logos::Logos;
use shared_types::{FloatSuffix, IntSuffix, Span};

use crate::errors::LexError;

use numbers::{
    parse_binary, parse_binary_suffix, parse_char, parse_decimal, parse_decimal_suffix,
    parse_float, parse_fractional_float_suffix, parse_hex, parse_hex_suffix, parse_octal,
    parse_octal_suffix,
};
use strings::{decode_string_literal, decode_triple_quoted_string};

mod numbers;
mod strings;

/// Carries both the numeric value and the explicit type suffix of a suffixed
/// integer literal (e.g. `42i64`, `255u8`).
///
/// See [`TokenKind::Integer`] for why the magnitude is a `u64`.
#[derive(Debug, Clone, PartialEq)]
pub struct IntegerSuffixToken {
    pub value: u64,
    pub suffix: IntSuffix,
}

/// Carries both the numeric value and the explicit type suffix of a suffixed
/// float literal (e.g. `1.5f32`, `2.0f64`, `1e10f32`).
#[derive(Debug, Clone, PartialEq)]
pub struct FloatSuffixToken {
    pub value: f64,
    pub suffix: FloatSuffix,
}

/// The decoded content of a string literal token: a plain literal, or the
/// text/hole chunks of an interpolated one.
///
/// One token variant for both shapes because a logos callback picks the
/// variant's *payload*, never the variant. The decoder decides
/// plain-vs-interpolated after it has walked the content.
#[derive(Debug, Clone, PartialEq)]
pub enum StringValue {
    /// No interpolation holes: the whole literal's decoded text.
    Plain(String),
    /// At least one `{expr}` hole; see [`InterpChunk`].
    Interp(Vec<InterpChunk>),
}

/// One segment of an interpolated string literal, as split by the lexer.
///
/// The lexer only locates the `{...}` holes (brace matching that skips char
/// literals) and hands each hole's raw source text to the parser, which
/// re-lexes and parses it as an expression. Keeping expression parsing out of
/// the lexer is what lets a hole contain calls, struct literals, and nested
/// blocks. A hole may not contain a `"` string literal: the quote ends the
/// enclosing token, so such a literal reports as an unterminated hole.
#[derive(Debug, Clone, PartialEq)]
pub enum InterpChunk {
    /// Literal text with escapes (`\n`, `\{`, …) already decoded.
    Text(String),
    /// A `{...}` hole: its raw source text and the absolute file span of that
    /// text (the braces themselves excluded), so diagnostics inside the hole
    /// point at the right column of the real file.
    Hole { source: String, span: Span },
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
    #[token("dyn")]
    Dyn,
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
    #[token("pool")]
    Pool,
    #[token("move")]
    Move,
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
    // suffixes; `bf16` precedes the others in the alternation only for
    // readability: logos matches the whole literal greedily regardless.
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
    /// The **magnitude** of an integer literal. A literal is never negative in
    /// source (`-1` is a negation over `1`), so the widest magnitude the language
    /// can spell is `u64::MAX`, and an `i64` here would reject both that and
    /// `9223372036854775808`, the magnitude `i64::MIN` is written with. Deciding
    /// what a magnitude means is the type checker's job, not the lexer's.
    Integer(u64),

    // String literals (including potentially malformed ones for better error messages).
    // All three patterns route through the same chunk decoder: a literal with no
    // `{...}` hole carries `StringValue::Plain`, one with holes carries
    // `StringValue::Interp`.
    //
    // The triple-quoted form is a bare `"""` token whose callback scans and bumps the
    // body itself. A regex cannot express it: logos has no non-greedy repetition, so a
    // pattern ending in `"""` would run to the LAST `"""` in the file. (0.16 parses a
    // lazy `*?` rather than rejecting it, but still matches greedily, so the lazy spelling
    // is not a way out of this: it silently swallows every literal in the file.) Matching only
    // the opening delimiter keeps the DFA trivial and hands the body to a hand-written
    // scanner. Three quotes always beat the two-quote empty-string match under logos'
    // longest-match rule, so `""` and `"""` never collide.
    #[token("\"\"\"", decode_triple_quoted_string)]
    #[regex(
        r#""([^"\\\n]|\\[nrt\\"0{xu]|\\u\{[0-9a-fA-F]+\}|\\x[0-9a-fA-F]{2})*""#,
        decode_string_literal,
        priority = 2
    )]
    #[regex(r#""([^"\\]|\\.)*""#, decode_string_literal, priority = 1)]
    String(StringValue),

    // Character literals: a single Unicode scalar value between single
    // quotes, e.g. `'a'`, `'\n'`, `'\u{1F44D}'`. The regex admits exactly one
    // content unit: a non-quote/backslash/newline char, a recognized escape, a
    // `\u{...}` unicode escape, or a `\xNN` byte escape. So `''`, `'ab'`, and an
    // unterminated `'a` never match and fall through to a lex error.
    #[regex(
        r"'([^'\\\n]|\\['nrt\\0]|\\u\{[0-9a-fA-F]+\}|\\x[0-9a-fA-F]{2})'",
        parse_char
    )]
    Char(char),

    // Lifetime name: a leading `'` followed by an identifier, with NO closing
    // quote (e.g. `'a` in `func longest<'a>(...)`). The callback strips the `'`, so the
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
    // PipeGreater must precede Pipe so logos longest-match picks `|>` over `|`
    #[token("|>")]
    PipeGreater,
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
    // Error propagation `expr?`. Declared after `??` for readability only: logos
    // matches the longest token, so `a ?? b` is never read as two propagations.
    #[token("?")]
    Question,

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

    // Comments and whitespace.
    // `allow_greedy` opts out of the 0.16 lint against unbounded greedy repetition.
    // The lint targets patterns that force a scan of the whole input per token; this
    // class excludes `\n`, so the run is bounded by the current line and a line
    // comment is exactly the "consume to end of line" this spells.
    #[regex(r"//[^\n]*", logos::skip, allow_greedy = true)]
    _LineComment,
    // Block comments NEST, which no regex can express: logos matches the
    // longest run its DFA accepts, so `/* a /* b */ c */` would close at the first
    // `*/` and leave ` c */` to lex as garbage. Matching only the opening delimiter
    // hands the body to a depth-counting scanner, the same shape `"""` uses.
    #[token("/*", lex_nested_block_comment)]
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
            TokenKind::Dyn => "dyn",
            TokenKind::Import => "import",
            TokenKind::Export => "export",
            TokenKind::Module => "module",
            TokenKind::Match => "match",
            TokenKind::Where => "where",
            TokenKind::Type => "type",
            TokenKind::Newtype => "newtype",
            TokenKind::Unsafe => "unsafe",
            TokenKind::Pool => "pool",
            TokenKind::Move => "move",
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
            TokenKind::PipeGreater => "|>",
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
            TokenKind::Question => "?",
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

/// Opening and closing delimiter of a block string literal.
const TRIPLE_QUOTE: &str = "\"\"\"";

/// Opening and closing delimiter of a block comment.
const BLOCK_COMMENT_OPEN: &[u8] = b"/*";
const BLOCK_COMMENT_CLOSE: &[u8] = b"*/";

/// Consume a nesting block comment, bumping the lexer past its closing delimiter.
///
/// Logos matched only the opening `/*`, so this counts depth over the remainder:
/// every further `/*` deepens it and every `*/` unwinds it, and the comment ends
/// when depth returns to zero. Delimiters inside string and char literals are NOT
/// exempt: a comment is scanned as raw text, matching how `//` already swallows a
/// quote to end of line.
fn lex_nested_block_comment(
    lex: &mut logos::Lexer<TokenKind>,
) -> logos::FilterResult<(), LexError> {
    let open = lex.span().start;
    let rest = lex.remainder().as_bytes();

    let mut depth = 1usize;
    let mut at = 0usize;
    while at < rest.len() {
        // Delimiters are two bytes and every non-ASCII UTF-8 byte is >= 0x80, so a
        // byte scan can never split a multi-byte character into a false `/` or `*`.
        if rest[at..].starts_with(BLOCK_COMMENT_OPEN) {
            depth += 1;
            at += BLOCK_COMMENT_OPEN.len();
            continue;
        }
        if rest[at..].starts_with(BLOCK_COMMENT_CLOSE) {
            at += BLOCK_COMMENT_CLOSE.len();
            depth -= 1;
            if depth == 0 {
                lex.bump(at);
                return logos::FilterResult::Skip;
            }
            continue;
        }
        at += 1;
    }

    logos::FilterResult::Error(LexError::UnterminatedBlockComment {
        span: Span::new(open, lex.source().len()),
    })
}

// ── Suffixed integer helpers ──────────────────────────────────────────────────

// ── Suffixed float helpers ────────────────────────────────────────────────────
