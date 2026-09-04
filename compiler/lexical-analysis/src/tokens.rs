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

/// The decoded content of a string literal token: a plain literal, or the
/// text/hole chunks of an interpolated one.
///
/// One token variant for both shapes because a logos callback picks the
/// variant's *payload*, never the variant — the decoder decides plain-vs-
/// interpolated after it has walked the content.
#[derive(Debug, Clone, PartialEq)]
pub enum StringValue {
    /// No interpolation holes — the whole literal's decoded text.
    Plain(String),
    /// At least one `{expr}` hole; see [`InterpChunk`].
    Interp(Vec<InterpChunk>),
}

/// One segment of an interpolated string literal, as split by the lexer.
///
/// The lexer only locates the `{...}` holes — brace matching that skips char
/// literals — and hands each hole's raw source text to the parser, which
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

    // String literals (including potentially malformed ones for better error messages).
    // All three patterns route through the same chunk decoder: a literal with no
    // `{...}` hole carries `StringValue::Plain`, one with holes carries
    // `StringValue::Interp`.
    //
    // The triple-quoted form is a bare `"""` token whose callback scans and bumps the
    // body itself. A regex cannot express it: logos has no non-greedy repetition, so a
    // pattern ending in `"""` would run to the LAST `"""` in the file. Matching only
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
    // content unit — a non-quote/backslash/newline char, a recognized escape, a
    // `\u{...}` unicode escape, or a `\xNN` byte escape — so `''`, `'ab'`, and an
    // unterminated `'a` never match and fall through to a lex error.
    #[regex(
        r"'([^'\\\n]|\\['nrt\\0]|\\u\{[0-9a-fA-F]+\}|\\x[0-9a-fA-F]{2})'",
        parse_char
    )]
    Char(char),

    // Lifetime name: a leading `'` followed by an identifier, with NO closing
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
    // Error propagation `expr?`. Declared after `??` for readability only — logos
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

    // Comments and whitespace
    #[regex(r"//[^\n]*", logos::skip)]
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
/// exempt — a comment is scanned as raw text, matching how `//` already swallows a
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

/// Decode a `"…"` string literal token, splitting interpolated literals into chunks.
///
/// This is the stateful half of string lexing: logos matches the literal's
/// shape, but finding where each `{...}` hole opens and closes requires walking
/// the content with a brace depth that skips over nested string/char literals —
/// beyond what a regular expression can express. A literal without any unescaped
/// `{` decodes exactly like the pre-interpolation lexer did ([`StringValue::Plain`]);
/// one with holes yields [`StringValue::Interp`] with raw hole sources for
/// the parser to re-parse.
fn decode_string_literal(lex: &mut logos::Lexer<TokenKind>) -> Result<StringValue, LexError> {
    let raw = lex.slice();
    let base = lex.span().start;
    let whole = Span::new(base, base + raw.len());
    let content = &raw[1..raw.len() - 1]; // Strip quotes
    let content_base = base + 1;
    let indexed: Vec<(usize, char)> = content
        .char_indices()
        .map(|(offset, ch)| (content_base + offset, ch))
        .collect();

    decode_chunks(lex.source(), &indexed, whole)
}

/// Scan, dedent, and decode a `"""…"""` block string literal.
///
/// Logos matched only the opening delimiter, so this walks the remainder for the
/// closing `"""`, bumps the lexer past it, applies the dedent rule, and hands the
/// surviving characters to the same chunk decoder ordinary literals use — escapes
/// and `{...}` holes therefore behave identically in both forms.
fn decode_triple_quoted_string(lex: &mut logos::Lexer<TokenKind>) -> Result<StringValue, LexError> {
    let source = lex.source();
    let open = lex.span().start;
    let body_start = lex.span().end;
    let rest = lex.remainder();

    let Some(close_offset) = find_triple_quote_close(rest) else {
        return Err(LexError::UnterminatedTripleQuotedString {
            span: Span::new(open, source.len()),
        });
    };
    lex.bump(close_offset + TRIPLE_QUOTE.len());

    let whole = Span::new(open, body_start + close_offset + TRIPLE_QUOTE.len());
    let indexed = dedent_block_body(&rest[..close_offset], body_start, whole)?;

    decode_chunks(source, &indexed, whole)
}

/// Byte offset of the `"""` that closes a block string, or `None` when it never closes.
///
/// Scanning bytes rather than chars is sound because every non-ASCII UTF-8 byte is
/// `>= 0x80` and so can never be mistaken for `\` or `"`.
fn find_triple_quote_close(body: &str) -> Option<usize> {
    let bytes = body.as_bytes();
    let mut at = 0usize;
    while at < bytes.len() {
        // An escape is opaque: `\"""` is a quote followed by the delimiter, not a
        // delimiter, and `\\` must not shield the quote that follows it.
        if bytes[at] == b'\\' {
            at += 2;
            continue;
        }
        if bytes[at..].starts_with(TRIPLE_QUOTE.as_bytes()) {
            return Some(at);
        }
        at += 1;
    }
    None
}

/// Strip the closing delimiter's indentation from every content line of a block
/// string, returning the surviving characters tagged with absolute source offsets.
///
/// Dedenting by dropping characters from the indexed vector — rather than by
/// rebuilding a `String` — is what keeps every remaining character's true offset, so
/// interpolation holes inside a block string still report at real source columns.
fn dedent_block_body(
    body: &str,
    body_start: usize,
    whole: Span,
) -> Result<Vec<(usize, char)>, LexError> {
    let Some(last_newline) = body.rfind('\n') else {
        return Err(LexError::TripleQuoteClosingNotOnOwnLine { span: whole });
    };
    let indent = &body[last_newline + 1..];
    if !indent.chars().all(is_horizontal_space) {
        return Err(LexError::TripleQuoteClosingNotOnOwnLine { span: whole });
    }

    let mut out: Vec<(usize, char)> = Vec::new();
    let mut offset = 0usize;
    let mut on_opening_line = true;

    while offset <= last_newline {
        let line_end = body[offset..]
            .find('\n')
            .map(|at| offset + at)
            .unwrap_or(last_newline);
        // A CRLF source ends each line with `\r\n`. The carriage return is line-ending
        // punctuation, not content: dropping it here is what makes a block string's
        // value identical whether the file was checked out with LF or CRLF endings.
        let line = &body[offset..line_end];
        let line = line.strip_suffix('\r').unwrap_or(line);

        if on_opening_line {
            // Whatever trails the opening `"""` sits flush against the delimiter and
            // cannot carry the closing indentation, so it is exempt from the dedent
            // rule. An empty remainder is punctuation: the newline goes with it.
            if !line.is_empty() {
                push_chars(&mut out, line, body_start + offset);
                out.push((body_start + line_end, '\n'));
            }
        } else if line.chars().all(is_horizontal_space) {
            // A blank line carries no indentation to check and normalizes to empty,
            // so a paragraph break never has to be padded out to the delimiter.
            out.push((body_start + line_end, '\n'));
        } else {
            let Some(stripped) = line.strip_prefix(indent) else {
                return Err(LexError::TripleQuoteUnderIndented {
                    indent: indent.chars().count(),
                    span: Span::new(body_start + offset, body_start + line_end),
                });
            };
            push_chars(&mut out, stripped, body_start + offset + indent.len());
            out.push((body_start + line_end, '\n'));
        }

        on_opening_line = false;
        offset = line_end + 1;
    }

    Ok(out)
}

fn push_chars(out: &mut Vec<(usize, char)>, text: &str, base: usize) {
    out.extend(text.char_indices().map(|(at, ch)| (base + at, ch)));
}

fn is_horizontal_space(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\r')
}

/// Split decoded string content into literal text and interpolation holes.
///
/// `indexed` carries each content character with its **absolute** offset in `source`,
/// so hole sources are sliced straight out of the file and their spans point at real
/// columns. Block strings exploit that: dedent simply omits the indentation
/// characters from `indexed`, leaving every survivor correctly located.
fn decode_chunks(
    source: &str,
    indexed: &[(usize, char)],
    whole: Span,
) -> Result<StringValue, LexError> {
    let mut parts: Vec<InterpChunk> = Vec::new();
    let mut text = String::new();
    let mut has_hole = false;
    let invalid_escape = |escape: String| LexError::InvalidEscape {
        escape,
        span: whole,
    };

    let mut i = 0usize;
    while i < indexed.len() {
        let (abs_off, ch) = indexed[i];

        if ch == '\\' {
            let Some((_, esc)) = indexed.get(i + 1).copied() else {
                return Err(LexError::UnterminatedString { span: whole });
            };
            match esc {
                'n' => {
                    text.push('\n');
                    i += 2;
                }
                'r' => {
                    text.push('\r');
                    i += 2;
                }
                't' => {
                    text.push('\t');
                    i += 2;
                }
                '\\' => {
                    text.push('\\');
                    i += 2;
                }
                '"' => {
                    text.push('"');
                    i += 2;
                }
                '0' => {
                    text.push('\0');
                    i += 2;
                }
                // Literal `{` / `}`: the interpolation delimiters' escape forms.
                '{' => {
                    text.push('{');
                    i += 2;
                }
                '}' => {
                    text.push('}');
                    i += 2;
                }
                'x' => {
                    let hex: Option<String> = indexed
                        .get(i + 2..i + 4)
                        .map(|pair| pair.iter().map(|(_, c)| c).collect());
                    let Some(hex) = hex.filter(|h| h.len() == 2) else {
                        return Err(invalid_escape("\\x".to_string()));
                    };
                    let code = u8::from_str_radix(&hex, 16)
                        .map_err(|_| invalid_escape(format!("\\x{}", hex)))?;
                    text.push(code as char);
                    i += 4;
                }
                'u' => {
                    if indexed.get(i + 2).map(|(_, c)| *c) != Some('{') {
                        return Err(invalid_escape("\\u".to_string()));
                    }
                    let mut hex = String::new();
                    let mut j = i + 3;
                    loop {
                        match indexed.get(j) {
                            Some((_, '}')) => break,
                            Some((_, c)) if c.is_ascii_hexdigit() => {
                                hex.push(*c);
                                j += 1;
                            }
                            _ => {
                                return Err(invalid_escape(format!("\\u{{{}}}", hex)));
                            }
                        }
                    }
                    let code = u32::from_str_radix(&hex, 16)
                        .map_err(|_| invalid_escape(format!("\\u{{{}}}", hex)))?;
                    let unicode_char = char::from_u32(code)
                        .ok_or_else(|| invalid_escape(format!("\\u{{{}}}", hex)))?;
                    text.push(unicode_char);
                    i = j + 1;
                }
                other => {
                    return Err(invalid_escape(format!("\\{}", other)));
                }
            }
            continue;
        }

        if ch == '{' {
            has_hole = true;
            let close_j = scan_hole_close(indexed, i)
                .ok_or(LexError::UnterminatedInterpolation { span: whole })?;

            if !text.is_empty() {
                parts.push(InterpChunk::Text(std::mem::take(&mut text)));
            }
            // `abs_off + 1` skips the `{`; the hole's span excludes both braces.
            let hole_start = abs_off + 1;
            let hole_end = indexed[close_j].0;
            parts.push(InterpChunk::Hole {
                source: source[hole_start..hole_end].to_string(),
                span: Span::new(hole_start, hole_end),
            });
            i = close_j + 1;
            continue;
        }

        // An unescaped `}` outside a hole is rejected rather than taken literally, so a
        // dropped `{` is caught where it goes missing instead of silently rendering the
        // rest of the hole as text. `\}` is the way to write the brace itself.
        if ch == '}' {
            return Err(LexError::UnescapedClosingBrace {
                span: Span::new(abs_off, abs_off + ch.len_utf8()),
            });
        }

        text.push(ch);
        i += 1;
    }

    if !has_hole {
        return Ok(StringValue::Plain(text));
    }
    if !text.is_empty() {
        parts.push(InterpChunk::Text(text));
    }
    Ok(StringValue::Interp(parts))
}

/// Find the index of the `}` closing the hole whose `{` sits at `open`, tracking
/// brace depth and skipping char literals so their escape braces do not count
/// (`"{'\u{7D}'}"`). Returns `None` when the hole never closes.
fn scan_hole_close(indexed: &[(usize, char)], open: usize) -> Option<usize> {
    let mut depth = 1usize;
    let mut j = open + 1;
    while j < indexed.len() {
        match indexed[j].1 {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(j);
                }
            }
            // A char literal's braces are data, not structure (`'\u{7D}'`).
            // Only `'` is reachable here: an unescaped `"` ends the string token
            // itself, so it never survives into a hole's content.
            '\'' => {
                // `skip_nested_literal` already lands one past the closing quote,
                // so re-enter the loop without the trailing bump.
                j = skip_nested_literal(indexed, j);
                continue;
            }
            _ => {}
        }
        j += 1;
    }
    None
}

/// Return the index just past the closing `'` of the char literal starting at
/// `open`. Backslash-skips honor escapes; a `\u{...}` payload may itself contain
/// quotes or braces (`'\u{7D}'`), so the whole escape is jumped rather than two
/// characters. An unterminated literal consumes the remainder — the caller
/// reports the enclosing hole as unterminated, which is the correct diagnosis
/// regardless.
fn skip_nested_literal(indexed: &[(usize, char)], open: usize) -> usize {
    let mut k = open + 1;
    while k < indexed.len() {
        let c = indexed[k].1;
        if c == '\\' {
            k += 2;
            if indexed.get(k - 1).map(|(_, c)| *c) == Some('u') {
                while k < indexed.len() && indexed[k].1 != '}' {
                    k += 1;
                }
                k += 1;
            }
            continue;
        }
        if c == '\'' {
            return k + 1;
        }
        k += 1;
    }
    indexed.len()
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

/// Parse a character literal into its single Unicode scalar value. The
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
