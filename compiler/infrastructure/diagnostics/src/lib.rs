// NEURO Programming Language - Diagnostics
// Infrastructure component for diagnostic message infrastructure

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

/// Diagnostic error codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticCode {
    SyntaxError,
    TypeError,
    NameError,
    Unknown,
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
}
