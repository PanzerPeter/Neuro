// Interpolated string literals: turning the lexer's text/hole chunks into an
// `Expr::InterpString` whose holes carry fully parsed expressions.

use lexical_analysis::{tokenize, InterpChunk, TokenKind};
use shared_types::{FormatAlign, FormatKind, FormatSpec, Span};

use crate::ast::{Expr, InterpPart};
use crate::errors::{ParseError, ParseResult};
use crate::precedence::Precedence;

use super::Parser;

/// Build the `Expr::InterpString` for an interpolated literal.
///
/// Each hole's raw source is re-lexed and parsed as a standalone expression by a
/// nested [`Parser`]. Token spans are shifted onto the hole's absolute file
/// coordinates *before* parsing, so every node inside a hole — and every
/// diagnostic later attached to one — points at the real column of the real file
/// rather than at an offset inside a detached snippet.
pub(super) fn parse_interp_string(chunks: &[InterpChunk], span: Span) -> ParseResult<Expr> {
    let mut parts = Vec::with_capacity(chunks.len());

    for chunk in chunks {
        match chunk {
            InterpChunk::Text(text) => parts.push(InterpPart::Text(text.clone())),
            InterpChunk::Hole {
                source,
                span: hole_span,
            } => parts.push(parse_hole(source, *hole_span)?),
        }
    }

    Ok(Expr::InterpString { parts, span })
}

/// Parse one `{expr}` or `{expr:spec}` hole.
fn parse_hole(source: &str, span: Span) -> ParseResult<InterpPart> {
    if source.trim().is_empty() {
        return Err(ParseError::EmptyInterpolationHole { span });
    }

    let mut tokens = tokenize(source)?;
    for token in &mut tokens {
        token.span = Span::new(token.span.start + span.start, token.span.end + span.start);
    }

    let mut parser = Parser::new(tokens);
    let expr = parser.parse_expr(Precedence::Lowest)?;

    let trailing = if parser.is_at_end() {
        None
    } else {
        parser.peek().cloned()
    };

    let spec = match trailing {
        None => None,
        Some(token) if token.kind == TokenKind::Colon => {
            // The spec is read from the raw text after the `:`, not from tokens:
            // `.2`, `08d`, and `<10` are not well-formed Neuro token sequences.
            let spec_start = token.span.end - span.start;
            let text = &source[spec_start..];
            Some(parse_format_spec(
                text,
                Span::new(token.span.end, span.end),
            )?)
        }
        Some(token) => {
            return Err(ParseError::UnexpectedToken {
                found: token.kind,
                expected: "':' followed by a format specifier, or the end of the interpolation"
                    .to_string(),
                span: token.span,
            })
        }
    };

    Ok(InterpPart::Formatted {
        expr: Box::new(expr),
        spec,
        span,
    })
}

/// Parse the `spec` half of `{expr:spec}` per the language's specifier table.
///
/// Grammar, every part optional but strictly ordered:
/// `[< > ^] [+] [0] [width] [. precision] [? e d x X b o]`
///
/// Applicability to the value's type is *not* decided here — the parser has no
/// types. This produces the written spec; semantic analysis rejects the
/// combinations that no type can satisfy (`{s:x}`, `{n:.2}`).
fn parse_format_spec(text: &str, span: Span) -> ParseResult<FormatSpec> {
    let invalid = |reason: &str| ParseError::InvalidFormatSpec {
        spec: text.to_string(),
        reason: reason.to_string(),
        span,
    };

    if text.is_empty() {
        return Err(invalid("a `:` must be followed by a format specifier"));
    }

    let chars: Vec<char> = text.chars().collect();
    let mut i = 0usize;
    let mut spec = FormatSpec::default();

    match chars.first() {
        Some('<') => {
            spec.align = Some(FormatAlign::Left);
            i += 1;
        }
        Some('>') => {
            spec.align = Some(FormatAlign::Right);
            i += 1;
        }
        Some('^') => {
            spec.align = Some(FormatAlign::Center);
            i += 1;
        }
        _ => {}
    }

    if chars.get(i) == Some(&'+') {
        spec.plus_sign = true;
        i += 1;
    }

    // A leading `0` is the zero-pad flag, never the first digit of the width:
    // `{n:08d}` pads to 8, and a width is meaningless with a leading zero anyway.
    if chars.get(i) == Some(&'0') {
        spec.zero_pad = true;
        i += 1;
    }

    let width_digits = take_digits(&chars, &mut i);
    if !width_digits.is_empty() {
        spec.width = Some(
            width_digits
                .parse::<u32>()
                .map_err(|_| invalid("field width does not fit in 32 bits"))?,
        );
    }

    if chars.get(i) == Some(&'.') {
        i += 1;
        let precision_digits = take_digits(&chars, &mut i);
        if precision_digits.is_empty() {
            return Err(invalid("`.` must be followed by a precision, as in `.2`"));
        }
        spec.precision = Some(
            precision_digits
                .parse::<u32>()
                .map_err(|_| invalid("precision does not fit in 32 bits"))?,
        );
    }

    if let Some(&letter) = chars.get(i) {
        spec.kind = match letter {
            '?' => FormatKind::Debug,
            'e' => FormatKind::Scientific,
            'd' => FormatKind::Decimal,
            'x' => FormatKind::LowerHex,
            'X' => FormatKind::UpperHex,
            'b' => FormatKind::Binary,
            'o' => FormatKind::Octal,
            _ => {
                return Err(invalid(
                    "expected one of `? e d x X b o` as the format kind",
                ))
            }
        };
        i += 1;
    } else if spec.precision.is_some() {
        // `.N` with no kind letter is fixed-point — the `{pi:.2}` row of the table.
        spec.kind = FormatKind::Fixed;
    }

    if i != chars.len() {
        return Err(invalid(
            "trailing characters after the format kind; the order is align, sign, zero, width, precision, kind",
        ));
    }

    Ok(spec)
}

/// Consume the run of ASCII digits at `i`, advancing it past them.
fn take_digits(chars: &[char], i: &mut usize) -> String {
    let mut digits = String::new();
    while let Some(c) = chars.get(*i).filter(|c| c.is_ascii_digit()) {
        digits.push(*c);
        *i += 1;
    }
    digits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(text: &str) -> FormatSpec {
        parse_format_spec(text, Span::new(0, text.len())).expect("spec parses")
    }

    #[test]
    fn bare_kind_letters_map_to_their_variants() {
        assert_eq!(spec("?").kind, FormatKind::Debug);
        assert_eq!(spec("d").kind, FormatKind::Decimal);
        assert_eq!(spec("x").kind, FormatKind::LowerHex);
        assert_eq!(spec("X").kind, FormatKind::UpperHex);
        assert_eq!(spec("b").kind, FormatKind::Binary);
        assert_eq!(spec("o").kind, FormatKind::Octal);
        assert_eq!(spec("e").kind, FormatKind::Scientific);
    }

    #[test]
    fn precision_without_kind_is_fixed_point() {
        let parsed = spec(".3");
        assert_eq!(parsed.kind, FormatKind::Fixed);
        assert_eq!(parsed.precision, Some(3));
    }

    #[test]
    fn precision_with_e_is_scientific() {
        let parsed = spec(".2e");
        assert_eq!(parsed.kind, FormatKind::Scientific);
        assert_eq!(parsed.precision, Some(2));
    }

    #[test]
    fn zero_flag_is_not_the_first_width_digit() {
        let parsed = spec("08d");
        assert!(parsed.zero_pad);
        assert_eq!(parsed.width, Some(8));
        assert_eq!(parsed.kind, FormatKind::Decimal);
    }

    #[test]
    fn alignment_precedes_width() {
        assert_eq!(spec("<10").align, Some(FormatAlign::Left));
        assert_eq!(spec(">10").align, Some(FormatAlign::Right));
        let centered = spec("^10");
        assert_eq!(centered.align, Some(FormatAlign::Center));
        assert_eq!(centered.width, Some(10));
    }

    #[test]
    fn plus_flag_parses_with_a_kind() {
        let parsed = spec("+d");
        assert!(parsed.plus_sign);
        assert_eq!(parsed.kind, FormatKind::Decimal);
    }

    #[test]
    fn empty_spec_is_rejected() {
        assert!(parse_format_spec("", Span::new(0, 0)).is_err());
    }

    #[test]
    fn unknown_kind_letter_is_rejected() {
        assert!(parse_format_spec("q", Span::new(0, 1)).is_err());
    }

    #[test]
    fn misordered_spec_is_rejected() {
        // Width after the kind letter, not before it.
        assert!(parse_format_spec("d10", Span::new(0, 3)).is_err());
    }

    #[test]
    fn dot_without_digits_is_rejected() {
        assert!(parse_format_spec(".", Span::new(0, 1)).is_err());
    }
}
