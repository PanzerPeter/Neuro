// Neuro Programming Language - AST Types
// Type AST nodes

use shared_types::{Identifier, Span};

/// Type AST nodes
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    /// Named type (e.g., i32, f64, String, bool)
    Named(Identifier),

    /// Borrow (reference) type (§2.4, §2.5): a non-owning reference to a value of
    /// type `inner`. `span` covers the leading `&` through the referent type.
    /// `mutable` distinguishes `&mut T` (write access) from `&T` (read-only).
    Reference {
        inner: Box<Type>,
        mutable: bool,
        span: Span,
    },

    /// Fixed-size array type `[T; N]` (§3.1): `N` elements of `element`, with `N`
    /// known at compile time. `span` covers the leading `[` through the closing `]`.
    Array {
        element: Box<Type>,
        size: usize,
        span: Span,
    },

    /// Anonymous tuple type `(T1, T2, ...)` (§3.2): a fixed-size, heterogeneous,
    /// positionally-indexed aggregate. `span` covers the leading `(` through the
    /// closing `)`. Always has at least two element types — a single
    /// parenthesized type is grouping, and the empty tuple (unit) is a separate
    /// concern not yet produced here.
    Tuple { elements: Vec<Type>, span: Span },

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
            Type::Array { span, .. } => *span,
            Type::Tuple { span, .. } => *span,
            Type::Tensor { span, .. } => *span,
        }
    }
}
