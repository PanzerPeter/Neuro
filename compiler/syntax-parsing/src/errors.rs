// Neuro Programming Language - Syntax Parsing
// Parse error definitions

use lexical_analysis::{LexError, TokenKind};
use shared_types::Span;
use thiserror::Error;

/// Parse errors
#[derive(Debug, Error, PartialEq)]
pub enum ParseError {
    #[error("unexpected token {found:?}, expected {expected}")]
    UnexpectedToken {
        found: TokenKind,
        expected: String,
        span: Span,
    },

    #[error("unexpected end of file, expected {expected}")]
    UnexpectedEof { expected: String },

    #[error("maximum expression nesting depth ({0}) exceeded - possible infinite recursion")]
    MaxDepthExceeded(usize),

    #[error("duplicate parameter name '{name}' in function definition")]
    DuplicateParameter { name: String, span: Span },

    #[error("duplicate type alias '{name}'")]
    DuplicateTypeAlias { name: String, span: Span },

    #[error("type alias '{name}' shadows a built-in type; choose a different name")]
    TypeAliasShadowsBuiltin { name: String, span: Span },

    #[error("type alias '{name}' is defined in terms of itself (cyclic alias)")]
    CyclicTypeAlias { name: String, span: Span },

    #[error("lexical error: {0}")]
    LexError(#[from] LexError),
}

/// Result type for parsing operations
pub type ParseResult<T> = Result<T, ParseError>;
