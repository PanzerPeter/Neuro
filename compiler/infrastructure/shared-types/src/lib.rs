//! Neuro Programming Language - Shared Types
//!
//! Infrastructure component providing common type definitions used across
//! compiler slices. This crate contains lightweight data structures for
//! representing source locations, identifiers, and literal values.
//!
//! # Architecture
//!
//! This is a pure infrastructure crate with no business logic. It provides
//! only data structures and simple operations that are universally needed
//! across the compiler.

/// Source code span representing a location in the source file.
///
/// A span is a half-open range `[start, end)` of byte offsets into the source text.
/// This is used throughout the compiler to track where AST nodes and tokens originated
/// from the source code, enabling accurate error reporting.
///
/// # Examples
///
/// ```
/// use shared_types::Span;
///
/// let span = Span::new(0, 5);
/// assert_eq!(span.start, 0);
/// assert_eq!(span.end, 5);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    /// Starting byte offset (inclusive)
    pub start: usize,
    /// Ending byte offset (exclusive)
    pub end: usize,
}

impl Span {
    /// Creates a new span from start and end byte offsets.
    ///
    /// # Examples
    ///
    /// ```
    /// use shared_types::Span;
    ///
    /// let span = Span::new(10, 20);
    /// assert_eq!(span.start, 10);
    /// assert_eq!(span.end, 20);
    /// ```
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// Merges two spans into a single span covering both ranges.
    ///
    /// The resulting span will start at the minimum of the two start positions
    /// and end at the maximum of the two end positions.
    ///
    /// # Examples
    ///
    /// ```
    /// use shared_types::Span;
    ///
    /// let span1 = Span::new(0, 5);
    /// let span2 = Span::new(3, 8);
    /// let merged = span1.merge(span2);
    /// assert_eq!(merged, Span::new(0, 8));
    /// ```
    pub fn merge(self, other: Self) -> Self {
        Self {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

/// Identifier representation with source location.
///
/// Used for variable names, function names, and other user-defined identifiers
/// in the source code. The span tracks where the identifier appears in the source.
///
/// # Examples
///
/// ```
/// use shared_types::{Identifier, Span};
///
/// let ident = Identifier::new("my_var".to_string(), Span::new(0, 6));
/// assert_eq!(ident.name, "my_var");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Identifier {
    /// The identifier name as it appears in the source code
    pub name: String,
    /// Source location of this identifier
    pub span: Span,
}

impl Identifier {
    /// Creates a new identifier with the given name and source span.
    pub fn new(name: String, span: Span) -> Self {
        Self { name, span }
    }
}

/// Type suffix on an integer literal (e.g., the `i64` in `42i64`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntSuffix {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
}

/// Type suffix on a float literal (e.g., the `f32` in `1.5f32`).
///
/// `F16`/`BF16` are the half-precision suffixes (`1.5f16`, `0.02bf16`, §1.2). The
/// suffix is the only way to write a half-precision literal — they have no
/// contextual default — because half-precision scalars carry a deliberately narrow
/// contract (storage, copy, equality, and `as`-cast only; no arithmetic).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloatSuffix {
    F16,
    BF16,
    F32,
    F64,
}

/// Literal value types supported in the language.
///
/// These represent constant values that appear directly in the source code.
/// The actual source location is typically tracked by the AST node containing
/// the literal, not by the literal itself.
#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    /// Integer literal, optionally suffixed (e.g., `42`, `42i64`, `255u8`).
    /// When the suffix is present it overrides contextual type inference.
    Integer(i64, Option<IntSuffix>),
    /// Floating-point literal, optionally suffixed (e.g., `3.14`, `1.5f32`, `2.0f64`).
    /// When the suffix is present it overrides contextual type inference.
    Float(f64, Option<FloatSuffix>),
    /// String literal (e.g., `"hello"`)
    String(String),
    /// Boolean literal (`true` or `false`)
    Boolean(bool),
    /// Character literal — a single Unicode scalar value (e.g. `'a'`, `'\n'`) (§1.2)
    Char(char),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_merge_works() {
        let span1 = Span::new(0, 5);
        let span2 = Span::new(3, 8);
        let merged = span1.merge(span2);
        assert_eq!(merged, Span::new(0, 8));
    }

    #[test]
    fn span_merge_non_overlapping() {
        let span1 = Span::new(0, 5);
        let span2 = Span::new(10, 15);
        let merged = span1.merge(span2);
        assert_eq!(merged, Span::new(0, 15));
    }

    #[test]
    fn span_merge_reversed() {
        let span1 = Span::new(10, 15);
        let span2 = Span::new(0, 5);
        let merged = span1.merge(span2);
        assert_eq!(merged, Span::new(0, 15));
    }

    #[test]
    fn span_equality() {
        let span1 = Span::new(5, 10);
        let span2 = Span::new(5, 10);
        let span3 = Span::new(5, 11);
        assert_eq!(span1, span2);
        assert_ne!(span1, span3);
    }

    #[test]
    fn identifier_creation() {
        let ident = Identifier::new("my_variable".to_string(), Span::new(0, 11));
        assert_eq!(ident.name, "my_variable");
        assert_eq!(ident.span, Span::new(0, 11));
    }

    #[test]
    fn identifier_equality() {
        let ident1 = Identifier::new("foo".to_string(), Span::new(0, 3));
        let ident2 = Identifier::new("foo".to_string(), Span::new(0, 3));
        let ident3 = Identifier::new("bar".to_string(), Span::new(0, 3));
        assert_eq!(ident1, ident2);
        assert_ne!(ident1, ident3);
    }

    #[test]
    fn literal_integer() {
        let lit = Literal::Integer(42, None);
        assert_eq!(lit, Literal::Integer(42, None));
        assert_ne!(lit, Literal::Integer(43, None));
    }

    #[test]
    fn literal_float() {
        let lit = Literal::Float(2.5, None);
        assert_eq!(lit, Literal::Float(2.5, None));
    }

    #[test]
    fn literal_float_suffixed() {
        let lit = Literal::Float(1.5, Some(FloatSuffix::F32));
        assert_eq!(lit, Literal::Float(1.5, Some(FloatSuffix::F32)));
        assert_ne!(lit, Literal::Float(1.5, Some(FloatSuffix::F64)));
        assert_ne!(lit, Literal::Float(1.5, None));
    }

    #[test]
    fn literal_string() {
        let lit = Literal::String("hello".to_string());
        assert_eq!(lit, Literal::String("hello".to_string()));
    }

    #[test]
    fn literal_boolean() {
        let lit_true = Literal::Boolean(true);
        let lit_false = Literal::Boolean(false);
        assert_eq!(lit_true, Literal::Boolean(true));
        assert_eq!(lit_false, Literal::Boolean(false));
        assert_ne!(lit_true, lit_false);
    }
}
