//! Lexical analysis error types
//! 
//! Error types and handling for the NEURO lexer, providing detailed 
//! error information for debugging and user feedback.

// Removed unused std::fmt import - using thiserror for Display impl
use thiserror::Error;

/// Errors that can occur during lexical analysis
#[derive(Debug, Error, Clone, PartialEq)]
pub enum LexError {
    /// Encountered an unexpected character
    #[error("Unexpected character '{char}' at line {line}, column {column}")]
    UnexpectedCharacter {
        char: char,
        line: usize,
        column: usize,
    },

    /// String literal was not properly terminated
    #[error("Unterminated string literal starting at position {start_position}")]
    UnterminatedString {
        start_position: usize,
    },

    /// Block comment was not properly terminated
    #[error("Unterminated block comment starting at position {start_position}")]
    UnterminatedBlockComment {
        start_position: usize,
    },

    /// Invalid escape sequence in string literal
    #[error("Invalid escape sequence '{sequence}' at line {line}, column {column}")]
    InvalidEscapeSequence {
        sequence: String,
        line: usize,
        column: usize,
    },

    /// Number literal is malformed
    #[error("Malformed number literal: {number}")]
    MalformedNumber {
        number: String,
    },

    /// Invalid Unicode character
    #[error("Invalid Unicode character at position {position}")]
    InvalidUnicode {
        position: usize,
    },
}

impl LexError {
    /// Get the error message suitable for display to users
    pub fn display_message(&self) -> String {
        match self {
            LexError::UnexpectedCharacter { char, line, column } => {
                format!("Unexpected character '{}' at line {}, column {}", char, line, column)
            }
            LexError::UnterminatedString { start_position } => {
                format!("Unterminated string literal starting at position {}", start_position)
            }
            LexError::UnterminatedBlockComment { start_position } => {
                format!("Unterminated block comment starting at position {}", start_position)
            }
            LexError::InvalidEscapeSequence { sequence, line, column } => {
                format!("Invalid escape sequence '{}' at line {}, column {}", sequence, line, column)
            }
            LexError::MalformedNumber { number } => {
                format!("Malformed number literal: {}", number)
            }
            LexError::InvalidUnicode { position } => {
                format!("Invalid Unicode character at position {}", position)
            }
        }
    }

    /// Get the error code for this error type
    pub fn error_code(&self) -> &'static str {
        match self {
            LexError::UnexpectedCharacter { .. } => "LEX001",
            LexError::UnterminatedString { .. } => "LEX002",
            LexError::UnterminatedBlockComment { .. } => "LEX003",
            LexError::InvalidEscapeSequence { .. } => "LEX004",
            LexError::MalformedNumber { .. } => "LEX005",
            LexError::InvalidUnicode { .. } => "LEX006",
        }
    }
}
