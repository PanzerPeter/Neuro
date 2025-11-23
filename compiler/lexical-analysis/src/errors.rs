// NEURO Programming Language - Lexical Analysis
// Lexical error definitions

use shared_types::Span;
use thiserror::Error;

/// Lexical analysis errors
#[derive(Debug, Error, Clone, PartialEq)]
pub enum LexError {
    #[error("unexpected character '{character}' at position {}", span.start)]
    UnexpectedChar { character: char, span: Span },

    #[error("unterminated string literal starting at position {}", span.start)]
    UnterminatedString { span: Span },

    #[error("invalid number literal '{text}' at position {}", span.start)]
    InvalidNumber { text: String, span: Span },

    #[error("invalid escape sequence '{escape}' at position {}", span.start)]
    InvalidEscape { escape: String, span: Span },

    #[error("unterminated block comment starting at position {}", span.start)]
    UnterminatedBlockComment { span: Span },
}

impl Default for LexError {
    fn default() -> Self {
        LexError::UnexpectedChar {
            character: '\0',
            span: Span::new(0, 0),
        }
    }
}

/// Result type for lexical analysis
pub type LexResult<T> = Result<T, LexError>;
