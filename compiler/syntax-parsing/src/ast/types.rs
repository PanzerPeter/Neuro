// NEURO Programming Language - Syntax Parsing
// Type AST nodes

use shared_types::{Identifier, Span};

/// Type AST nodes
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Named(Identifier),
    Tensor {
        element_type: Box<Type>,
        shape: Vec<usize>,
        span: Span,
    },
}
