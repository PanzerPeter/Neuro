//! Diagnostic reporting and collection
//!
//! Provides utilities for collecting and reporting diagnostics.

use crate::{Diagnostic, Severity};
use std::fmt;

/// A collection of diagnostics with reporting capabilities
#[derive(Debug, Clone, Default)]
pub struct DiagnosticReport {
    diagnostics: Vec<Diagnostic>,
}

impl DiagnosticReport {
    /// Create a new empty diagnostic report
    pub fn new() -> Self {
        Self {
            diagnostics: Vec::new(),
        }
    }

    /// Add a diagnostic to this report
    pub fn add(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    /// Add an error diagnostic
    pub fn error(&mut self, message: impl Into<String>) {
        self.add(Diagnostic::error(message));
    }

    /// Add a warning diagnostic
    pub fn warning(&mut self, message: impl Into<String>) {
        self.add(Diagnostic::warning(message));
    }

    /// Add an info diagnostic
    pub fn info(&mut self, message: impl Into<String>) {
        self.add(Diagnostic::info(message));
    }

    /// Get all diagnostics
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Check if there are any errors
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }

    /// Check if there are any warnings
    pub fn has_warnings(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Warning)
    }

    /// Get count of errors
    pub fn error_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .count()
    }

    /// Get count of warnings
    pub fn warning_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Warning)
            .count()
    }

    /// Check if the report is empty
    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    /// Clear all diagnostics
    pub fn clear(&mut self) {
        self.diagnostics.clear();
    }

    /// Merge another report into this one
    pub fn merge(&mut self, other: DiagnosticReport) {
        self.diagnostics.extend(other.diagnostics);
    }

    /// Sort diagnostics by severity (errors first)
    pub fn sort_by_severity(&mut self) {
        self.diagnostics.sort_by_key(|d| d.severity);
    }
}

impl fmt::Display for DiagnosticReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.diagnostics.is_empty() {
            return write!(f, "No diagnostics");
        }

        for (i, diagnostic) in self.diagnostics.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            write!(f, "{}", diagnostic)?;
        }

        if self.has_errors() || self.has_warnings() {
            writeln!(f)?;
            write!(f, "Summary: ")?;
            if self.has_errors() {
                write!(f, "{} error(s)", self.error_count())?;
                if self.has_warnings() {
                    write!(f, ", ")?;
                }
            }
            if self.has_warnings() {
                write!(f, "{} warning(s)", self.warning_count())?;
            }
        }

        Ok(())
    }
}

impl From<Vec<Diagnostic>> for DiagnosticReport {
    fn from(diagnostics: Vec<Diagnostic>) -> Self {
        Self { diagnostics }
    }
}

impl From<Diagnostic> for DiagnosticReport {
    fn from(diagnostic: Diagnostic) -> Self {
        Self {
            diagnostics: vec![diagnostic],
        }
    }
}
