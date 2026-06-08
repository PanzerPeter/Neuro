// Neuro Programming Language - AST Types
// Type AST nodes

use shared_types::{Identifier, Span};

/// Type AST nodes
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    /// Named type (e.g., i32, f64, String, bool)
    Named(Identifier),

    /// Immutable borrow type `&T` (§2.4): a non-owning reference to a value of
    /// type `inner`. `span` covers the leading `&` through the referent type.
    Reference { inner: Box<Type>, span: Span },

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

impl Type {
    /// Get the span of this type annotation
    pub fn span(&self) -> Span {
        match self {
            Type::Named(ident) => ident.span,
            Type::Reference { span, .. } => *span,
            Type::Tensor { span, .. } => *span,
        }
    }
}
