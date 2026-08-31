// Parse error definitions

use lexical_analysis::{LexError, TokenKind};
use shared_types::Span;
use thiserror::Error;

/// Parse errors
#[derive(Debug, Error, PartialEq)]
pub enum ParseError {
    #[error("unexpected token {found:?}, expected {expected}")]
    UnexpectedToken {
        found: TokenKind,
        expected: String,
        span: Span,
    },

    #[error("unexpected end of file, expected {expected}")]
    UnexpectedEof { expected: String },

    #[error("maximum expression nesting depth ({0}) exceeded - possible infinite recursion")]
    MaxDepthExceeded(usize),

    #[error("duplicate parameter name '{name}' in function definition")]
    DuplicateParameter { name: String, span: Span },

    #[error("two parameters share the call-site name '{label}'; a named argument must identify exactly one parameter")]
    DuplicateParameterLabel { label: String, span: Span },

    #[error("duplicate type alias '{name}'")]
    DuplicateTypeAlias { name: String, span: Span },

    #[error("type alias '{name}' shadows a built-in type; choose a different name")]
    TypeAliasShadowsBuiltin { name: String, span: Span },

    #[error("type alias '{name}' is defined in terms of itself (cyclic alias)")]
    CyclicTypeAlias { name: String, span: Span },

    #[error("enum '{name}' may not declare lifetime parameters; enum payloads are restricted to scalar types, so a borrowed payload has nothing to annotate")]
    EnumLifetimeParam { name: String, span: Span },

    #[error("`export` cannot be applied to {what}")]
    ExportNotAllowed { what: String, span: Span },

    #[error("`@no_prelude` must be the first thing in a file; it opts that file out of the implicit prelude, so it cannot follow a declaration or sit inside a `module` block")]
    MisplacedNoPrelude { span: Span },

    #[error("an interpolation hole `{{}}` must contain an expression")]
    EmptyInterpolationHole { span: Span },

    #[error("invalid format specifier `{spec}`: {reason}")]
    InvalidFormatSpec {
        spec: String,
        reason: String,
        span: Span,
    },

    #[error("string interpolation is not allowed in a pattern; a pattern must be a constant")]
    InterpolationInPattern { span: Span },

    #[error("a `for` head binding a pair `(index, value)` iterates an enumerated sequence; add `.enumerate()` to the iterable")]
    PairWithoutEnumerate { span: Span },

    #[error("`.enumerate()` yields a position and a value; bind both with a pair pattern `for (index, value) in ...`")]
    EnumerateWithoutPair { span: Span },

    #[error("`.enumerate()` takes no arguments")]
    EnumerateTakesNoArguments { span: Span },

    #[error("lexical error: {0}")]
    LexError(#[from] LexError),
}

/// Result type for parsing operations
pub type ParseResult<T> = Result<T, ParseError>;
