// Lint warnings emitted alongside successful type checking.

use shared_types::Span;
use std::fmt;

/// Stable identifier for a lint warning. The kebab-case rendering is what
/// users write in `@allow(...)` to suppress the warning, with `-` rewritten
/// as `_` since `-` is not an identifier character.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarningCode {
    /// `while true { ... }` should be written as `loop { ... }` (§3.7).
    PreferLoopOverWhileTrue,
}

impl WarningCode {
    /// The user-facing kebab-case name used in diagnostic output.
    pub fn name(self) -> &'static str {
        match self {
            WarningCode::PreferLoopOverWhileTrue => "prefer-loop-over-while-true",
        }
    }

    /// The identifier accepted inside `@allow(...)` to suppress this warning.
    /// Identifiers cannot contain `-`, so the kebab-case name is mapped to
    /// snake_case here.
    pub fn allow_identifier(self) -> &'static str {
        match self {
            WarningCode::PreferLoopOverWhileTrue => "prefer_loop_over_while_true",
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
