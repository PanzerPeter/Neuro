//! Diagnostic and error reporting infrastructure
//!
//! Provides common error handling and diagnostic capabilities
//! for all NEURO compiler slices.

pub mod diagnostic;
pub mod report;
pub mod severity;

pub use diagnostic::*;
pub use report::*;
pub use severity::*;

use shared_types::Span;
use std::fmt;