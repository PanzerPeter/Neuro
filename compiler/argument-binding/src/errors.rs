// Diagnostics for named-argument binding.

use shared_types::Span;
use thiserror::Error;

/// A call site whose arguments cannot be bound to the callee's parameters.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum ArgumentError {
    #[error("in the call to '{callee}': a positional argument cannot follow a named one; move it before `{label}:`")]
    PositionalAfterNamed {
        callee: String,
        label: String,
        span: Span,
    },

    #[error("'{callee}' has no parameter named '{label}'")]
    UnknownArgumentLabel {
        callee: String,
        label: String,
        span: Span,
    },

    #[error("argument '{label}' is given twice in the call to '{callee}'")]
    DuplicateArgumentLabel {
        callee: String,
        label: String,
        span: Span,
    },

    #[error("the '{label}' argument of '{callee}' must be named: write `{label}: <value>`")]
    MissingArgumentLabel {
        callee: String,
        label: String,
        span: Span,
    },

    #[error("'{callee}' takes {expected} argument(s), but {found} given")]
    ArgumentCountMismatch {
        callee: String,
        expected: usize,
        found: usize,
        span: Span,
    },

    #[error("the '{label}' parameter of '{callee}' is declared `_ {label}:`, which means it is passed positionally and its name is not written at the call site")]
    SuppressedLabel {
        callee: String,
        label: String,
        span: Span,
    },

    #[error("named arguments are not available here: '{callee}' has no declared parameter names")]
    LabelsUnsupported { callee: String, span: Span },

    #[error("named arguments cannot be used with '{callee}': more than one type declares a method of that name with different parameter names, so `{label}:` does not identify one parameter")]
    AmbiguousMethodLabels {
        callee: String,
        label: String,
        span: Span,
    },
}

impl ArgumentError {
    /// Where the offending call site is, for a caller rendering the diagnostic.
    pub fn span(&self) -> Span {
        match self {
            ArgumentError::PositionalAfterNamed { span, .. }
            | ArgumentError::UnknownArgumentLabel { span, .. }
            | ArgumentError::DuplicateArgumentLabel { span, .. }
            | ArgumentError::MissingArgumentLabel { span, .. }
            | ArgumentError::ArgumentCountMismatch { span, .. }
            | ArgumentError::SuppressedLabel { span, .. }
            | ArgumentError::LabelsUnsupported { span, .. }
            | ArgumentError::AmbiguousMethodLabels { span, .. } => *span,
        }
    }
}
