use thiserror::Error;

/// Failures that can arise while constructing or verifying MLIR through melior.
#[derive(Debug, Error)]
pub enum MlirError {
    /// MLIR's own verifier rejected the constructed module.
    #[error("MLIR module failed verification")]
    ModuleVerificationFailed,

    /// A melior call (block argument access, operation result access, ...) failed.
    #[error("melior operation failed: {0}")]
    Melior(#[from] melior::Error),
}
