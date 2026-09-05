//! Common type definitions shared across compiler slices: source locations,
//! identifiers, and literal values. Pure infrastructure with no business logic.

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
/// `F16`/`BF16` are the half-precision suffixes (`1.5f16`, `0.02bf16`). The
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

/// Alignment inside a padded interpolation field: `:<10`, `:>10`, `:^10`.
///
/// The language's specifier table has no fill character — padding fills with
/// spaces (or zeros under the `0` flag), so alignment is the whole story.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatAlign {
    Left,
    Right,
    Center,
}

/// The base rendering an interpolation hole selects with its format kind letter
/// (`{pi:.2}`, `{n:x}`, `{s:?}`). The letters mirror the language's specifier table;
/// applicability to a value's type is checked later, against the resolved type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatKind {
    /// No kind letter — Display-style default per type.
    Default,
    /// `?` — debug rendering. Scalars only this phase; aggregate support awaits
    /// `@derive(Debug)`.
    Debug,
    /// `.N` — fixed-point, N decimal places (floats).
    Fixed,
    /// `e` / `.Ne` — scientific notation with the exponent normalized to
    /// `3.14e0` form (floats).
    Scientific,
    /// `d` — decimal (integers).
    Decimal,
    /// `x` — lowercase hexadecimal (integers).
    LowerHex,
    /// `X` — uppercase hexadecimal (integers).
    UpperHex,
    /// `b` — binary (integers).
    Binary,
    /// `o` — octal (integers).
    Octal,
}

/// The parsed `spec` half of an interpolated hole `{expr:spec}`.
///
/// Pure data: the grammar shape is validated where the spec is written into the
/// AST (the parser), and applicability to the value's type where the type is
/// known (semantic analysis). Every pass between them only reads fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatSpec {
    pub align: Option<FormatAlign>,
    /// `0` flag — zero-pad to `width`. Rejected together with [`FormatAlign::Left`]
    /// (one forces digits left, the other right).
    pub zero_pad: bool,
    /// `+` flag — always render the sign for numeric values.
    pub plus_sign: bool,
    /// Field width; shorter renders are padded to it.
    pub width: Option<u32>,
    /// `.N` precision. Meaningful only for [`FormatKind::Fixed`] /
    /// [`FormatKind::Scientific`]; the checker rejects it elsewhere.
    pub precision: Option<u32>,
    pub kind: FormatKind,
}

impl Default for FormatSpec {
    fn default() -> Self {
        Self {
            align: None,
            zero_pad: false,
            plus_sign: false,
            width: None,
            precision: None,
            kind: FormatKind::Default,
        }
    }
}

/// Sanity ceiling on a written field width (`{x:<99999}` is legal source but a
/// program that pads to megabytes per evaluation is almost certainly a typo).
/// Checked in semantic analysis so the error carries a span.
pub const MAX_FORMAT_WIDTH: u32 = 4096;

/// Sanity ceiling on `.N` precision. Comfortably above what an `f64` can even
/// express (fixed notation of the smallest subnormal needs ~324 places), so the
/// cap only rejects absurd requests, never legitimate ones.
pub const MAX_FORMAT_PRECISION: u32 = 1024;

/// Literal value types supported in the language.
///
/// These represent constant values that appear directly in the source code.
/// The actual source location is typically tracked by the AST node containing
/// the literal, not by the literal itself.
#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    /// Integer literal, optionally suffixed (e.g., `42`, `42i64`, `255u8`).
    /// When the suffix is present it overrides contextual type inference.
    ///
    /// Carried as an `i128` because no narrower type spans every integer the
    /// language can spell: `u64::MAX` does not fit an `i64`, and `i64::MIN` is
    /// written as a negation over the magnitude `9223372036854775808`, which does
    /// not either. A magnitude is always non-negative as it leaves the lexer; the
    /// sign appears only where a negation is folded into the literal (match
    /// patterns), and the checker range-checks against the type's own bounds.
    Integer(i128, Option<IntSuffix>),
    /// Floating-point literal, optionally suffixed (e.g., `3.14`, `1.5f32`, `2.0f64`).
    /// When the suffix is present it overrides contextual type inference.
    Float(f64, Option<FloatSuffix>),
    /// String literal (e.g., `"hello"`)
    String(String),
    /// Boolean literal (`true` or `false`)
    Boolean(bool),
    /// Character literal — a single Unicode scalar value (e.g. `'a'`, `'\n'`)
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
    fn format_spec_default_is_display() {
        let spec = FormatSpec::default();
        assert_eq!(spec.kind, FormatKind::Default);
        assert_eq!(spec.align, None);
        assert!(!spec.zero_pad);
        assert!(!spec.plus_sign);
        assert_eq!(spec.width, None);
        assert_eq!(spec.precision, None);
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
