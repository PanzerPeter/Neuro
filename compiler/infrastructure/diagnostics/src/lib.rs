//! NEURO Programming Language - Diagnostics
//!
//! Infrastructure component for collecting and formatting compiler diagnostic messages
//! (errors, warnings, hints, and informational messages).
//!
//! # Overview
//!
//! This crate provides:
//! - Diagnostic severity levels (Error, Warning, Info, Hint)
//! - Diagnostic error codes for categorization
//! - Builder pattern for constructing diagnostics with spans and notes
//! - Diagnostic collector for accumulating multiple diagnostics
//!
//! # Architecture
//!
//! Pure infrastructure with no business logic. Used throughout the compiler
//! for error reporting and user feedback.
//!
//! # Examples
//!
//! ```
//! use diagnostics::{Diagnostic, DiagnosticCode, DiagnosticCollector};
//! use shared_types::Span;
//!
//! let mut collector = DiagnosticCollector::new();
//!
//! collector.add(
//!     Diagnostic::error(DiagnosticCode::TypeError, "type mismatch".to_string())
//!         .with_span(Span::new(10, 15))
//!         .with_note("expected i32, found f64".to_string())
//! );
//!
//! if collector.has_errors() {
//!     for diag in collector.diagnostics() {
//!         eprintln!("{}", diag);
//!     }
//! }
//! ```

use shared_types::Span;
use thiserror::Error;

/// Diagnostic severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Error => write!(f, "error"),
            Severity::Warning => write!(f, "warning"),
            Severity::Info => write!(f, "info"),
            Severity::Hint => write!(f, "hint"),
        }
    }
}

/// Diagnostic error codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticCode {
    SyntaxError,
    TypeError,
    NameError,
    Unknown,
}

impl std::fmt::Display for DiagnosticCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiagnosticCode::SyntaxError => write!(f, "E0001"),
            DiagnosticCode::TypeError => write!(f, "E0002"),
            DiagnosticCode::NameError => write!(f, "E0003"),
            DiagnosticCode::Unknown => write!(f, "E0000"),
        }
    }
}

/// A diagnostic message
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: DiagnosticCode,
    pub message: String,
    pub span: Option<Span>,
    pub notes: Vec<String>,
}

impl Diagnostic {
    pub fn error(code: DiagnosticCode, message: String) -> Self {
        Self {
            severity: Severity::Error,
            code,
            message,
            span: None,
            notes: Vec::new(),
        }
    }

    pub fn warning(code: DiagnosticCode, message: String) -> Self {
        Self {
            severity: Severity::Warning,
            code,
            message,
            span: None,
            notes: Vec::new(),
        }
    }

    pub fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }

    pub fn with_note(mut self, note: String) -> Self {
        self.notes.push(note);
        self
    }
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.severity)?;
        write!(f, "[{}]", self.code)?;

        if let Some(span) = self.span {
            write!(f, " at {}..{}", span.start, span.end)?;
        }

        write!(f, ": {}", self.message)?;

        for note in &self.notes {
            write!(f, "\n  note: {}", note)?;
        }

        Ok(())
    }
}

/// Diagnostic collector
#[derive(Debug, Default)]
pub struct DiagnosticCollector {
    diagnostics: Vec<Diagnostic>,
}

impl DiagnosticCollector {
    pub fn new() -> Self {
        Self {
            diagnostics: Vec::new(),
        }
    }

    pub fn add(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn into_diagnostics(self) -> Vec<Diagnostic> {
        self.diagnostics
    }
}

/// Common diagnostic errors
#[derive(Debug, Error)]
pub enum DiagnosticError {
    #[error("compilation failed with {0} error(s)")]
    CompilationFailed(usize),

    #[error("internal compiler error: {0}")]
    InternalError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_collector_tracks_errors() {
        let mut collector = DiagnosticCollector::new();
        assert!(!collector.has_errors());

        collector.add(Diagnostic::error(
            DiagnosticCode::SyntaxError,
            "unexpected token".to_string(),
        ));

        assert!(collector.has_errors());
        assert_eq!(collector.diagnostics().len(), 1);
    }

    #[test]
    fn diagnostic_display_without_span() {
        let diag = Diagnostic::error(DiagnosticCode::TypeError, "type mismatch".to_string());
        let output = format!("{}", diag);
        assert_eq!(output, "error[E0002]: type mismatch");
    }

    #[test]
    fn diagnostic_display_with_span() {
        let diag = Diagnostic::error(DiagnosticCode::SyntaxError, "unexpected token".to_string())
            .with_span(Span::new(10, 15));
        let output = format!("{}", diag);
        assert_eq!(output, "error[E0001] at 10..15: unexpected token");
    }

    #[test]
    fn diagnostic_display_with_notes() {
        let diag = Diagnostic::warning(DiagnosticCode::Unknown, "unused variable".to_string())
            .with_note("consider using underscore prefix".to_string())
            .with_note("or remove the variable".to_string());
        let output = format!("{}", diag);
        assert_eq!(
            output,
            "warning[E0000]: unused variable\n  note: consider using underscore prefix\n  note: or remove the variable"
        );
    }

    #[test]
    fn severity_display() {
        assert_eq!(format!("{}", Severity::Error), "error");
        assert_eq!(format!("{}", Severity::Warning), "warning");
        assert_eq!(format!("{}", Severity::Info), "info");
        assert_eq!(format!("{}", Severity::Hint), "hint");
    }

    #[test]
    fn diagnostic_code_display() {
        assert_eq!(format!("{}", DiagnosticCode::SyntaxError), "E0001");
        assert_eq!(format!("{}", DiagnosticCode::TypeError), "E0002");
        assert_eq!(format!("{}", DiagnosticCode::NameError), "E0003");
        assert_eq!(format!("{}", DiagnosticCode::Unknown), "E0000");
    }
}
