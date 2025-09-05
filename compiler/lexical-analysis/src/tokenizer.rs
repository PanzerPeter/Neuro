//! High-level tokenizer interface
//! 
//! Provides a simplified interface for tokenizing NEURO source code,
//! building on top of the lexer implementation.

use crate::{Lexer, LexError};
use shared_types::{Token, Span};

/// High-level tokenizer for NEURO source code
pub struct Tokenizer {
    pub source: String,
    pub file_path: Option<String>,
}

impl Tokenizer {
    /// Create a new tokenizer with the given source code
    pub fn new(source: String) -> Self {
        Self {
            source,
            file_path: None,
        }
    }

    /// Create a new tokenizer with source code and file path
    pub fn with_file(source: String, file_path: String) -> Self {
        Self {
            source,
            file_path: Some(file_path),
        }
    }

    /// Tokenize the source code into a vector of tokens
    pub fn tokenize(&self) -> Result<Vec<Token>, LexError> {
        let mut lexer = Lexer::new(&self.source);
        lexer.tokenize()
    }

    /// Tokenize and filter out newline tokens for easier parsing
    pub fn tokenize_filtered(&self) -> Result<Vec<Token>, LexError> {
        let tokens = self.tokenize()?;
        Ok(tokens.into_iter()
            .filter(|token| !matches!(token.token_type, shared_types::TokenType::Newline))
            .collect())
    }

    /// Get a specific line from the source code for error reporting
    pub fn get_line(&self, line_number: usize) -> Option<&str> {
        if line_number == 0 {
            return None;
        }
        self.source
            .lines()
            .nth(line_number - 1)
    }

    /// Get source text for a given span
    pub fn get_text(&self, span: &Span) -> Option<&str> {
        if span.end <= self.source.len() {
            Some(&self.source[span.start..span.end])
        } else {
            None
        }
    }

    /// Create a tokenizer from a file path
    pub fn from_file(file_path: &str) -> std::io::Result<Self> {
        let source = std::fs::read_to_string(file_path)?;
        Ok(Self::with_file(source, file_path.to_string()))
    }
}
