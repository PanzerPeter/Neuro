// Feature slice for tokenization and lexical processing.
// Public API: `tokenize()`.

mod errors;
mod tokens;

pub use errors::{LexError, LexResult};
pub use tokens::{
    FloatSuffixToken, IntegerSuffixToken, InterpChunk, StringValue, Token, TokenKind,
};

use logos::Logos;
use shared_types::Span;

/// Turn logos' catch-all error into the diagnostic it stands for. logos reports any
/// unmatched input as `UnexpectedChar('\0')`; an unclosed `"` is the one case worth
/// its own message, and every other one gets the character actually found.
fn classify_error(source: &str, err: LexError, span: Span) -> LexError {
    match err {
        LexError::UnexpectedChar {
            character: '\0', ..
        } => {
            let start = span.start;
            let remaining = source.get(start..).unwrap_or_default();
            if remaining.starts_with('"') {
                let end = remaining
                    .find('\n')
                    .map(|offset| start + offset)
                    .unwrap_or(source.len());

                return LexError::UnterminatedString {
                    span: Span::new(start, end),
                };
            }

            let character = remaining.chars().next().unwrap_or('\0');
            LexError::UnexpectedChar { character, span }
        }
        other => other,
    }
}

/// Tokenize Neuro source into a token stream terminated by an `Eof` token.
///
/// The main entry point for lexical analysis; returns early on the first
/// lexical error (invalid character, unterminated string, etc.).
///
/// # Examples
///
/// ```
/// use lexical_analysis::tokenize;
///
/// fn main() {
///     let source = "func add(a: i32, b: i32) -> i32 { return a + b }";
///     let tokens = tokenize(source).unwrap();
/// }
/// ```
pub fn tokenize(source: &str) -> LexResult<Vec<Token>> {
    let mut tokens = Vec::new();

    for (kind, range) in TokenKind::lexer(source).spanned() {
        let span = Span::new(range.start, range.end);
        let kind = kind.map_err(|err| classify_error(source, err, span))?;
        tokens.push(Token::new(kind, span));
    }

    let eof_span = Span::new(source.len(), source.len());
    tokens.push(Token::new(TokenKind::Eof, eof_span));

    Ok(tokens)
}

#[cfg(test)]
mod tests;
