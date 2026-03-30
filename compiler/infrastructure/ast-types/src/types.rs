// NEURO Programming Language - AST Types
// Type AST nodes

use shared_types::{Identifier, Span};

/// Type AST nodes
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    /// Named type (e.g., i32, f64, String, bool)
    Named(Identifier),

    /// Tensor type for multi-dimensional arrays.
    ///
    /// This variant is reserved for future language support and is not yet
    /// produced by the parser.
    /// Example target syntax: `Tensor<f32, [3, 3]>`.
    Tensor {
        element_type: Box<Type>,
        shape: Vec<usize>,
        span: Span,
    },
}
