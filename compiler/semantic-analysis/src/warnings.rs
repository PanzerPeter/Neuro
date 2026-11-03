// Lint warnings emitted alongside successful type checking.

use shared_types::Span;
use std::fmt;

/// Stable identifier for a lint warning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarningCode {
    /// `while true { ... }` should be written as `loop { ... }`.
    PreferLoopOverWhileTrue,
    /// A constant float cast with `as` to an integer type it does not fit, so the
    /// cast saturates (or maps NaN to zero) where the author may have expected the value.
    FloatCastOutOfRange,
}

impl WarningCode {
    /// The lint's one name: printed in the warning and accepted by `@allow(...)`, so
    /// the name a reader sees is the one they write to silence it.
    pub fn name(self) -> &'static str {
        match self {
            WarningCode::PreferLoopOverWhileTrue => "prefer_loop_over_while_true",
            WarningCode::FloatCastOutOfRange => "float_cast_out_of_range",
        }
    }
}

/// A non-fatal diagnostic produced by semantic analysis. The presence of any
/// `Warning` does not block compilation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    pub code: WarningCode,
    pub message: String,
    pub span: Span,
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "warning[{}] at {}..{}: {}",
            self.code.name(),
            self.span.start,
            self.span.end,
            self.message
        )
    }
}
