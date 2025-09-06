//! NEURO Compiler (neurc) - Command Line Interface
//!
//! This slice implements the complete CLI for the NEURO compiler,
//! integrating lexical analysis, parsing, and module resolution
//! following VSA (Vertical Slice Architecture) principles.

pub mod cli;
pub mod driver;
pub mod output;
pub mod commands;

pub use cli::*;
pub use driver::*;
pub use output::*;
pub use commands::*;
