// Lexical error definitions

use shared_types::Span;
use thiserror::Error;

/// Lexical analysis errors
#[derive(Debug, Error, Clone, PartialEq)]
pub enum LexError {
    #[error("unexpected character '{character}'")]
    UnexpectedChar { character: char, span: Span },

    #[error("unterminated string literal")]
    UnterminatedString { span: Span },

    #[error("invalid number literal '{text}'")]
    InvalidNumber { text: String, span: Span },

    #[error("invalid escape sequence '{escape}'")]
    InvalidEscape { escape: String, span: Span },

    #[error("invalid character literal {literal}")]
    InvalidCharLiteral { literal: String, span: Span },

    #[error("unterminated block comment")]
    UnterminatedBlockComment { span: Span },

    #[error(
        "interpolation hole opened with `{{` is never closed: add the matching `}}` (or escape the brace as `\\{{`)"
    )]
    UnterminatedInterpolation { span: Span },

    #[error("unterminated triple-quoted string: add a closing `\"\"\"` on its own line")]
    UnterminatedTripleQuotedString { span: Span },

    #[error(
        "closing `\"\"\"` must be on its own line: move it to the next line and indent it to the level you want stripped"
    )]
    TripleQuoteClosingNotOnOwnLine { span: Span },

    #[error(
        "line in triple-quoted string is indented less than the closing `\"\"\"` ({indent} columns): indent every content line to at least the closing delimiter"
    )]
    TripleQuoteUnderIndented { indent: usize, span: Span },

    #[error(
        "unescaped `}}` is outside an interpolation hole: write `\\}}` for a literal brace, or add the `{{` this one was meant to close"
    )]
    UnescapedClosingBrace { span: Span },
}

impl LexError {
    /// The source range the error points at.
    pub fn span(&self) -> Span {
        match self {
            LexError::UnexpectedChar { span, .. }
            | LexError::UnterminatedString { span }
            | LexError::InvalidNumber { span, .. }
            | LexError::InvalidEscape { span, .. }
            | LexError::InvalidCharLiteral { span, .. }
            | LexError::UnterminatedBlockComment { span }
            | LexError::UnterminatedInterpolation { span }
            | LexError::UnterminatedTripleQuotedString { span }
            | LexError::TripleQuoteClosingNotOnOwnLine { span }
            | LexError::TripleQuoteUnderIndented { span, .. }
            | LexError::UnescapedClosingBrace { span } => *span,
        }
    }

    /// The same error with its span moved `offset` bytes later.
    pub(crate) fn shifted(mut self, offset: usize) -> Self {
        match &mut self {
            LexError::UnexpectedChar { span, .. }
            | LexError::UnterminatedString { span }
            | LexError::InvalidNumber { span, .. }
            | LexError::InvalidEscape { span, .. }
            | LexError::InvalidCharLiteral { span, .. }
            | LexError::UnterminatedBlockComment { span }
            | LexError::UnterminatedInterpolation { span }
            | LexError::UnterminatedTripleQuotedString { span }
            | LexError::TripleQuoteClosingNotOnOwnLine { span }
            | LexError::TripleQuoteUnderIndented { span, .. }
            | LexError::UnescapedClosingBrace { span } => *span = shift(*span, offset),
        }
        self
    }
}

/// `span` moved `offset` bytes later.
pub(crate) fn shift(span: Span, offset: usize) -> Span {
    Span::new(span.start + offset, span.end + offset)
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
