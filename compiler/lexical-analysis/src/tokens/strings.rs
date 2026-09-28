//! Decoding string literals: escapes, triple-quoted blocks and their dedent, and the
//! text and hole chunks of an interpolated string.

use shared_types::Span;

use crate::errors::LexError;

use super::{InterpChunk, StringValue, TokenKind, TRIPLE_QUOTE};

/// Decode a `"…"` string literal token, splitting interpolated literals into chunks.
///
/// This is the stateful half of string lexing: logos matches the literal's
/// shape, but finding where each `{...}` hole opens and closes requires walking
/// the content with a brace depth that skips over nested string/char literals,
/// beyond what a regular expression can express. A literal without any unescaped
/// `{` decodes exactly like the pre-interpolation lexer did ([`StringValue::Plain`]);
/// one with holes yields [`StringValue::Interp`] with raw hole sources for
/// the parser to re-parse.
pub(super) fn decode_string_literal(
    lex: &mut logos::Lexer<TokenKind>,
) -> Result<StringValue, LexError> {
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
/// surviving characters to the same chunk decoder ordinary literals use, so escapes
/// and `{...}` holes behave identically in both forms.
pub(super) fn decode_triple_quoted_string(
    lex: &mut logos::Lexer<TokenKind>,
) -> Result<StringValue, LexError> {
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
pub(super) fn find_triple_quote_close(body: &str) -> Option<usize> {
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
/// string, drop the newline that precedes the closing line, and return the surviving
/// characters tagged with absolute source offsets.
///
/// Dedenting by dropping characters from the indexed vector (rather than by
/// rebuilding a `String`) is what keeps every remaining character's true offset, so
/// interpolation holes inside a block string still report at real source columns.
pub(super) fn dedent_block_body(
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

    // Every content line pushed its own terminator, so the last one carries the
    // newline that separates it from the closing delimiter's line. That newline is
    // punctuation belonging to the delimiter, not content: dropping it is what makes
    // a block "no leading or trailing blank". A trailing newline is still writable:
    // leave a blank line before the closer and its terminator becomes the last one.
    if out.last().is_some_and(|(_, ch)| *ch == '\n') {
        out.pop();
    }

    Ok(out)
}

pub(super) fn push_chars(out: &mut Vec<(usize, char)>, text: &str, base: usize) {
    out.extend(text.char_indices().map(|(at, ch)| (base + at, ch)));
}

pub(super) fn is_horizontal_space(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\r')
}

/// Split decoded string content into literal text and interpolation holes.
///
/// `indexed` carries each content character with its **absolute** offset in `source`,
/// so hole sources are sliced straight out of the file and their spans point at real
/// columns. Block strings exploit that: dedent simply omits the indentation
/// characters from `indexed`, leaving every survivor correctly located.
pub(super) fn decode_chunks(
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
pub(super) fn scan_hole_close(indexed: &[(usize, char)], open: usize) -> Option<usize> {
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
/// characters. An unterminated literal consumes the remainder, and the caller
/// reports the enclosing hole as unterminated, which is the correct diagnosis
/// regardless.
pub(super) fn skip_nested_literal(indexed: &[(usize, char)], open: usize) -> usize {
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
