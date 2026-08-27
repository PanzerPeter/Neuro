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

    #[error("invalid character literal {literal} at position {}", span.start)]
    InvalidCharLiteral { literal: String, span: Span },

    #[error("unterminated block comment starting at position {}", span.start)]
    UnterminatedBlockComment { span: Span },

    #[error("interpolation hole opened with `{{` is never closed at position {}: add the matching `}}` (or escape the brace as `\\{{`)", span.start)]
    UnterminatedInterpolation { span: Span },

    #[error("unterminated triple-quoted string starting at position {}: add a closing `\"\"\"` on its own line", span.start)]
    UnterminatedTripleQuotedString { span: Span },

    #[error("closing `\"\"\"` at position {} must be on its own line: move it to the next line and indent it to the level you want stripped", span.start)]
    TripleQuoteClosingNotOnOwnLine { span: Span },

    #[error("line in triple-quoted string at position {} is indented less than the closing `\"\"\"` ({indent} columns): indent every content line to at least the closing delimiter", span.start)]
    TripleQuoteUnderIndented { indent: usize, span: Span },
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
