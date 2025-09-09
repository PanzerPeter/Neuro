//! NEURO Lexical Analysis
//! 
//! This slice handles tokenization and lexical processing of NEURO source code.
//! It follows VSA principles by being self-contained and focused on the single
//! business capability of converting raw source text into tokens.

pub mod lexer;
pub mod tokenizer;
pub mod error;

#[cfg(test)]
pub mod test_edge_cases;

pub use lexer::*;
pub use tokenizer::*;
pub use error::*;

use shared_types::{Token, TokenType, Span};