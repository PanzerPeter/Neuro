//! Diagnostic types and functionality
//!
//! Provides structured diagnostic reporting for the NEURO compiler.

use serde::{Deserialize, Serialize};
use source_location::Span;
use std::fmt;

/// Diagnostic severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

/// A diagnostic message with location and severity
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Diagnostic {
    /// The severity of this diagnostic
    pub severity: Severity,
    /// The main error message
    pub message: String,
    /// The location where this diagnostic applies
    pub span: Option<Span>,
    /// Error code for categorization
    pub code: Option<String>,
    /// Additional contextual information
    pub notes: Vec<String>,
    /// Related diagnostics
    pub related: Vec<Diagnostic>,
}

impl Diagnostic {
    /// Create a new error diagnostic
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            message: message.into(),
            span: None,
            code: None,
            notes: Vec::new(),
            related: Vec::new(),
        }
    }

    /// Create a new warning diagnostic
    pub fn warning(message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            message: message.into(),
            span: None,
            code: None,
            notes: Vec::new(),
            related: Vec::new(),
        }
    }

    /// Create a new info diagnostic
    pub fn info(message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Info,
            message: message.into(),
            span: None,
            code: None,
            notes: Vec::new(),
            related: Vec::new(),
        }
    }

    /// Set the span for this diagnostic
    pub fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }

    /// Set the error code for this diagnostic
    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    /// Add a note to this diagnostic
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    /// Add a related diagnostic
    pub fn with_related(mut self, related: Diagnostic) -> Self {
        self.related.push(related);
        self
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Error => write!(f, "error"),
            Severity::Warning => write!(f, "warning"),
            Severity::Info => write!(f, "info"),
            Severity::Hint => write!(f, "hint"),
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.severity, self.message)?;

        if let Some(ref code) = self.code {
            write!(f, " [{}]", code)?;
        }

        if let Some(ref span) = self.span {
            write!(f, " at {:?}", span)?;
        }

        for note in &self.notes {
            write!(f, "\n  note: {}", note)?;
        }

        Ok(())
    }
}
