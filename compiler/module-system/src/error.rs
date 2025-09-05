//! Module system error types

use thiserror::Error;
use std::path::PathBuf;

/// Errors that can occur in the module system
#[derive(Error, Debug, Clone)]
pub enum ModuleError {
    #[error("Module not found: {0}")]
    ModuleNotFound(String),

    #[error("Import path could not be resolved: {path}")]
    ImportResolutionFailed { path: String },

    #[error("Circular dependency detected in modules: {modules:?}")]
    CircularDependency { modules: Vec<String> },

    #[error("File not found: {path}")]
    FileNotFound { path: PathBuf },

    #[error("Failed to read file {path}: {error}")]
    FileReadError { path: PathBuf, error: String },

    #[error("Invalid module path: {path}")]
    InvalidModulePath { path: String },

    #[error("Module already registered: {path}")]
    DuplicateModule { path: String },
}