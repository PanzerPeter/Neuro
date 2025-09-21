//! Diagnostic and error reporting infrastructure
//!
//! Provides common error handling and diagnostic capabilities
//! for all NEURO compiler slices.

pub mod diagnostic;
pub mod report;
pub mod severity;

// Re-exports available but unused in current implementation
// pub use diagnostic::*;
// pub use report::*;
// pub use severity::*;