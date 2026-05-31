// NEURO Programming Language - Lexical Analysis
// Feature slice for tokenization and lexical processing
//
// This slice follows Vertical Slice Architecture (VSA) principles:
// - Self-contained tokenization functionality
// - Minimal dependencies (only infrastructure)
// - Clear module boundaries with pub(crate) for internals
// - Public API: tokenize() and Lexer struct

mod errors;
mod tokens;

// Public exports
pub use errors::{LexError, LexResult};
pub use tokens::{FloatSuffixToken, IntegerSuffixToken, Token, TokenKind};

use logos::Logos;
use shared_types::Span;

/// Lexer for the NEURO language
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

        // Check first character
        let Some(first) = chars.next() else {
            return false;
        };

        if first != '_' && !unicode_ident::is_xid_start(first) {
            return false;
        }

        // Check remaining characters
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

/// Convenience function to tokenize NEURO source code
///
/// This is the main entry point for lexical analysis. It takes NEURO source code
/// and produces a stream of tokens. Returns early on the first lexical error.
///
/// # Arguments
///
/// * `source` - The NEURO source code as a string
///
/// # Returns
///
/// * `Ok(Vec<Token>)` - Successfully tokenized source (includes EOF token)
/// * `Err(LexError)` - First lexical error encountered (invalid character, unterminated string, etc.)
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

    // Collect tokens, returning early on first error
    for result in lexer {
        tokens.push(result?);
    }

    // Add EOF token
    let eof_span = Span::new(source.len(), source.len());
    tokens.push(Token::new(TokenKind::Eof, eof_span));

    Ok(tokens)
}

#[cfg(test)]
mod tests;
