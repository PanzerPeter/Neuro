//! Literal value types

use serde::{Deserialize, Serialize};
use std::fmt;

/// Literal values in NEURO source code
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Literal {
    Integer(i64),
    Float(f64),
    String(String),
    Boolean(bool),
    Tensor(TensorLiteral),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TensorLiteral {
    pub values: Vec<Literal>,
    pub shape: Vec<usize>,
}

impl fmt::Display for Literal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Literal::Integer(i) => write!(f, "{}", i),
            Literal::Float(fl) => write!(f, "{}", fl),
            Literal::String(s) => write!(f, "\"{}\"", s),
            Literal::Boolean(b) => write!(f, "{}", b),
            Literal::Tensor(t) => write!(f, "Tensor{:?}", t.shape),
        }
    }
}