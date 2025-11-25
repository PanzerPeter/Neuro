// NEURO Programming Language - Syntax Parsing
// Type AST nodes

use shared_types::{Identifier, Span};

/// Type AST nodes
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    /// Named type (e.g., i32, f64, String, bool)
    Named(Identifier),

    /// Tensor type for multi-dimensional arrays
    ///
    /// TODO(Phase 2): Implement tensor type parsing
    /// This variant is defined for future use but not currently parsed.
    /// Tensor syntax will be added in Phase 2 when implementing ML features.
    /// Example: Tensor<f32, [3, 3]> for a 3x3 matrix of floats
    Tensor {
        element_type: Box<Type>,
        shape: Vec<usize>,
        span: Span,
    },
}
