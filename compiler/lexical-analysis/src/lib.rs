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

// Removed unused imports - types are re-exported from sub-modules