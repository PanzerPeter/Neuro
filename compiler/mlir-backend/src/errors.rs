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

    /// The conversion pipeline that rewrites the module into the `llvm` dialect
    /// failed, either because a pass errored or because the post-pass verifier
    /// rejected the result.
    #[error("MLIR conversion to the llvm dialect failed")]
    PassPipelineFailed,

    /// `mlirTranslateModuleToLLVMIR` returned no module. The module reached it
    /// carrying an operation outside the `llvm` dialect, or one with no
    /// registered LLVM translation interface.
    #[error("MLIR could not be translated to LLVM IR")]
    TranslationFailed,

    /// LLVM's own verifier rejected the translated module. Distinct from
    /// [`MlirError::ModuleVerificationFailed`], which is MLIR's verifier on the
    /// pre-translation module.
    #[error("translated LLVM module failed verification: {0}")]
    LlvmVerificationFailed(String),

    /// A melior call (block argument access, operation result access, ...) failed.
    #[error("melior operation failed: {0}")]
    Melior(#[from] melior::Error),
}
