use thiserror::Error;

/// Failures that can arise while constructing or verifying MLIR through melior.
#[derive(Debug, Error)]
pub enum MlirError {
    /// MLIR's own verifier rejected the constructed module.
    #[error("MLIR module failed verification")]
    ModuleVerificationFailed,

    /// A HIR type with no MLIR scaffold mapping appeared in value position
    /// (e.g. `void` as a parameter type).
    #[error("unsupported HIR type for MLIR lowering: {0}")]
    UnsupportedType(String),

    /// A melior call (block argument access, operation result access, ...) failed.
    #[error("melior operation failed: {0}")]
    Melior(#[from] melior::Error),
}
