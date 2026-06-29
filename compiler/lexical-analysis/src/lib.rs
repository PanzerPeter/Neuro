// Feature slice for tokenization and lexical processing.
// Public API: the `Lexer` struct and `tokenize()`.

mod errors;
mod tokens;

pub use errors::{LexError, LexResult};
pub use tokens::{FloatSuffixToken, IntegerSuffixToken, Token, TokenKind};

use logos::Logos;
use shared_types::Span;

/// Lexer for the Neuro language
pub struct Lexer<'source> {
    source: &'source str,
    inner: logos::Lexer<'source, TokenKind>,
}

impl<'source> Lexer<'source> {
    /// Create a new lexer for the given source code
    pub fn new(source: &'source str) -> Self {
        Self {
            source,
            inner: TokenKind::lexer(source),
        }
    }

    fn classify_error(&self, err: LexError, span: Span) -> LexError {
        match err {
            LexError::UnexpectedChar {
                character: '\0', ..
            } => {
                let start = span.start;
                if let Some(remaining) = self.source.get(start..) {
                    if remaining.starts_with('"') {
                        let end = remaining
                            .find('\n')
                            .map(|offset| start + offset)
                            .unwrap_or(self.source.len());

                        return LexError::UnterminatedString {
                            span: Span::new(start, end),
                        };
                    }
                }

                let character = self.inner.slice().chars().next().unwrap_or('\0');
                LexError::UnexpectedChar { character, span }
            }
            other => other,
        }
    }

    /// Check if a string is a valid identifier
    ///
    /// Follows Unicode XID standard: first character must be XID_Start or underscore,
    /// remaining characters must be XID_Continue.
    pub fn is_valid_identifier(s: &str) -> bool {
        let mut chars = s.chars();

        let Some(first) = chars.next() else {
            return false;
        };

        if first != '_' && !unicode_ident::is_xid_start(first) {
            return false;
        }

        chars.all(unicode_ident::is_xid_continue)
    }
}

impl<'source> Iterator for Lexer<'source> {
    type Item = LexResult<Token>;

    fn next(&mut self) -> Option<Self::Item> {
        let kind = self.inner.next()?;
        let span = Span::new(self.inner.span().start, self.inner.span().end);

        Some(match kind {
            Ok(kind) => Ok(Token::new(kind, span)),
            Err(err) => Err(self.classify_error(err, span)),
        })
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
    let lexer = Lexer::new(source);
    let mut tokens = Vec::new();

    for result in lexer {
        tokens.push(result?);
    }

    let eof_span = Span::new(source.len(), source.len());
    tokens.push(Token::new(TokenKind::Eof, eof_span));

    Ok(tokens)
}

#[cfg(test)]
mod tests;
