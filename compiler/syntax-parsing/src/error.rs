//! Parser error types
//! 
//! Error types and handling for the NEURO parser, providing detailed 
//! error information for syntax errors and recovery.

use shared_types::{TokenType, Span};
use thiserror::Error;

/// Errors that can occur during parsing
#[derive(Debug, Error, Clone, PartialEq)]
pub enum ParseError {
    /// Unexpected token during parsing
    #[error("Unexpected token {found:?}, expected {expected:?} at {span:?}")]
    UnexpectedToken {
        found: TokenType,
        expected: Vec<TokenType>,
        span: Span,
    },

    /// Unexpected end of file
    #[error("Unexpected end of file, expected {expected:?}")]
    UnexpectedEof {
        expected: Vec<TokenType>,
    },

    /// Invalid expression
    #[error("Invalid expression at {span:?}: {message}")]
    InvalidExpression {
        message: String,
        span: Span,
    },

    /// Invalid statement
    #[error("Invalid statement at {span:?}: {message}")]
    InvalidStatement {
        message: String,
        span: Span,
    },

    /// Invalid type annotation
    #[error("Invalid type annotation at {span:?}: {message}")]
    InvalidType {
        message: String,
        span: Span,
    },

    /// Missing semicolon
    #[error("Missing semicolon after statement at {span:?}")]
    MissingSemicolon {
        span: Span,
    },

    /// Unclosed delimiter (parentheses, brackets, braces)
    #[error("Unclosed {delimiter} starting at {start:?}")]
    UnclosedDelimiter {
        delimiter: String,
        start: Span,
    },

    /// Invalid number literal
    #[error("Invalid number literal '{literal}' at {span:?}")]
    InvalidNumber {
        literal: String,
        span: Span,
    },
}

impl ParseError {
    /// Create a new UnexpectedToken error
    pub fn unexpected_token(found: TokenType, expected: Vec<TokenType>, span: Span) -> Self {
        Self::UnexpectedToken {
            found,
            expected,
            span,
        }
    }

    /// Create a new UnexpectedEof error
    pub fn unexpected_eof(expected: Vec<TokenType>) -> Self {
        Self::UnexpectedEof { expected }
    }

    /// Create a new InvalidExpression error
    pub fn invalid_expression(message: impl Into<String>, span: Span) -> Self {
        Self::InvalidExpression {
            message: message.into(),
            span,
        }
    }

    /// Create a new InvalidStatement error
    pub fn invalid_statement(message: impl Into<String>, span: Span) -> Self {
        Self::InvalidStatement {
            message: message.into(),
            span,
        }
    }

    /// Get the error code for this error type
    pub fn error_code(&self) -> &'static str {
        match self {
            ParseError::UnexpectedToken { .. } => "PARSE001",
            ParseError::UnexpectedEof { .. } => "PARSE002",
            ParseError::InvalidExpression { .. } => "PARSE003",
            ParseError::InvalidStatement { .. } => "PARSE004",
            ParseError::InvalidType { .. } => "PARSE005",
            ParseError::MissingSemicolon { .. } => "PARSE006",
            ParseError::UnclosedDelimiter { .. } => "PARSE007",
            ParseError::InvalidNumber { .. } => "PARSE008",
        }
    }

    /// Get the span where this error occurred, if available
    pub fn span(&self) -> Option<Span> {
        match self {
            ParseError::UnexpectedToken { span, .. } => Some(*span),
            ParseError::InvalidExpression { span, .. } => Some(*span),
            ParseError::InvalidStatement { span, .. } => Some(*span),
            ParseError::InvalidType { span, .. } => Some(*span),
            ParseError::MissingSemicolon { span } => Some(*span),
            ParseError::UnclosedDelimiter { start, .. } => Some(*start),
            ParseError::InvalidNumber { span, .. } => Some(*span),
            ParseError::UnexpectedEof { .. } => None,
        }
    }
}