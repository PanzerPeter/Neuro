//! Decoding numeric and character literals: each radix, the integer and float type
//! suffixes, and `char` escapes.

use shared_types::{FloatSuffix, IntSuffix, Span};

use crate::errors::LexError;

use super::{FloatSuffixToken, IntegerSuffixToken, TokenKind};

/// Helper function to parse float literals
pub(super) fn parse_float(lex: &mut logos::Lexer<TokenKind>) -> Result<f64, LexError> {
    let slice = lex.slice().replace('_', "");
    slice.parse::<f64>().map_err(|_| LexError::InvalidNumber {
        text: lex.slice().to_string(),
        span: Span::new(lex.span().start, lex.span().end),
    })
}

/// Helper function to parse decimal integer literals
pub(super) fn parse_decimal(lex: &mut logos::Lexer<TokenKind>) -> Result<u64, LexError> {
    let slice = lex.slice().replace('_', "");
    slice.parse::<u64>().map_err(|_| LexError::InvalidNumber {
        text: lex.slice().to_string(),
        span: Span::new(lex.span().start, lex.span().end),
    })
}

/// Helper function to parse binary integer literals
pub(super) fn parse_binary(lex: &mut logos::Lexer<TokenKind>) -> Result<u64, LexError> {
    let slice = lex.slice()[2..].replace('_', ""); // Skip "0b" prefix
    u64::from_str_radix(&slice, 2).map_err(|_| LexError::InvalidNumber {
        text: lex.slice().to_string(),
        span: Span::new(lex.span().start, lex.span().end),
    })
}

/// Helper function to parse octal integer literals
pub(super) fn parse_octal(lex: &mut logos::Lexer<TokenKind>) -> Result<u64, LexError> {
    let slice = lex.slice()[2..].replace('_', ""); // Skip "0o" prefix
    u64::from_str_radix(&slice, 8).map_err(|_| LexError::InvalidNumber {
        text: lex.slice().to_string(),
        span: Span::new(lex.span().start, lex.span().end),
    })
}

/// Helper function to parse hexadecimal integer literals
pub(super) fn parse_hex(lex: &mut logos::Lexer<TokenKind>) -> Result<u64, LexError> {
    let slice = lex.slice()[2..].replace('_', ""); // Skip "0x" prefix
    u64::from_str_radix(&slice, 16).map_err(|_| LexError::InvalidNumber {
        text: lex.slice().to_string(),
        span: Span::new(lex.span().start, lex.span().end),
    })
}

/// Parse a character literal into its single Unicode scalar value. The
/// regex guarantees exactly one content unit between the quotes; this decodes a
/// recognized escape (`\n`, `\u{...}`, `\xNN`, …) or returns the lone character.
/// A `\u{...}` payload outside the valid scalar range (e.g. a surrogate) is the
/// one case the regex cannot reject, so it is validated here.
pub(super) fn parse_char(lex: &mut logos::Lexer<TokenKind>) -> Result<char, LexError> {
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
            // `\u{NNNN}`: the regex shape is fixed, so skip the leading `{` and
            // read hex digits until `}`.
            let hex: String = chars.take_while(|&c| c != '}').skip(1).collect();
            let code = u32::from_str_radix(&hex, 16).map_err(|_| invalid(content))?;
            char::from_u32(code).ok_or_else(|| invalid(content))
        }
        _ => Err(invalid(content)),
    }
}

/// Maps the suffix string (e.g. "i64") to `IntSuffix`. Panics for unexpected
/// inputs: the logos regex guarantees the suffix is one of the eight variants.
pub(super) fn parse_int_suffix(suffix: &str) -> IntSuffix {
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

pub(super) fn parse_decimal_suffix(
    lex: &mut logos::Lexer<TokenKind>,
) -> Result<IntegerSuffixToken, LexError> {
    let raw = lex.slice();
    let suffix_start = raw.find(|c: char| c.is_alphabetic()).unwrap_or(raw.len());
    let digits = raw[..suffix_start].replace('_', "");
    let value = digits.parse::<u64>().map_err(|_| LexError::InvalidNumber {
        text: raw.to_string(),
        span: Span::new(lex.span().start, lex.span().end),
    })?;
    Ok(IntegerSuffixToken {
        value,
        suffix: parse_int_suffix(&raw[suffix_start..]),
    })
}

pub(super) fn parse_binary_suffix(
    lex: &mut logos::Lexer<TokenKind>,
) -> Result<IntegerSuffixToken, LexError> {
    let raw = lex.slice();
    let suffix_start = raw[2..]
        .find(|c: char| c.is_alphabetic())
        .map(|i| i + 2)
        .unwrap_or(raw.len());
    let digits = raw[2..suffix_start].replace('_', "");
    let value = u64::from_str_radix(&digits, 2).map_err(|_| LexError::InvalidNumber {
        text: raw.to_string(),
        span: Span::new(lex.span().start, lex.span().end),
    })?;
    Ok(IntegerSuffixToken {
        value,
        suffix: parse_int_suffix(&raw[suffix_start..]),
    })
}

pub(super) fn parse_octal_suffix(
    lex: &mut logos::Lexer<TokenKind>,
) -> Result<IntegerSuffixToken, LexError> {
    let raw = lex.slice();
    let suffix_start = raw[2..]
        .find(|c: char| c.is_alphabetic())
        .map(|i| i + 2)
        .unwrap_or(raw.len());
    let digits = raw[2..suffix_start].replace('_', "");
    let value = u64::from_str_radix(&digits, 8).map_err(|_| LexError::InvalidNumber {
        text: raw.to_string(),
        span: Span::new(lex.span().start, lex.span().end),
    })?;
    Ok(IntegerSuffixToken {
        value,
        suffix: parse_int_suffix(&raw[suffix_start..]),
    })
}

/// Splits a suffixed float literal into its digit portion and `FloatSuffix`.
///
/// `bf16` is checked before `f16` because `"...bf16"` also ends in `"f16"`;
/// stripping the shorter suffix first would leave a stray `b` in the digits.
pub(super) fn split_float_suffix(raw: &str) -> Option<(&str, FloatSuffix)> {
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
pub(super) fn parse_fractional_float_suffix(
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

pub(super) fn parse_hex_suffix(
    lex: &mut logos::Lexer<TokenKind>,
) -> Result<IntegerSuffixToken, LexError> {
    let raw = lex.slice();
    // Skip "0x" prefix; find first alphabetic that is NOT a hex digit (a-f/A-F)
    let after_prefix = &raw[2..];
    let suffix_start = after_prefix
        .find(|c: char| c.is_alphabetic() && !matches!(c, 'a'..='f' | 'A'..='F'))
        .map(|i| i + 2)
        .unwrap_or(raw.len());
    let digits = raw[2..suffix_start].replace('_', "");
    let value = u64::from_str_radix(&digits, 16).map_err(|_| LexError::InvalidNumber {
        text: raw.to_string(),
        span: Span::new(lex.span().start, lex.span().end),
    })?;
    Ok(IntegerSuffixToken {
        value,
        suffix: parse_int_suffix(&raw[suffix_start..]),
    })
}
