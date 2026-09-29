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

    /// MLIR rejected an attribute this backend generated from its textual form.
    /// The text is produced by the lowering itself, never by a user, so this is a
    /// compiler bug surfaced as a value rather than left to panic.
    #[error("MLIR rejected the generated attribute `{0}`")]
    AttributeSyntax(String),

    /// A GPU chip name that is not letters, digits and `_`. It is spliced into a
    /// textual pass pipeline, so anything else could rewrite the pipeline.
    #[error("invalid GPU chip name `{0}`: expected letters, digits and `_`, such as `sm_80` or `gfx90a`")]
    InvalidGpuChip(String),

    /// `gpu-module-to-binary` produced no device object for a kernel module. An AMD
    /// code object is linked by ROCm's `ld.lld`, found under `$ROCM_PATH/llvm/bin`.
    #[error("GPU kernels could not be serialized; an AMD target needs ROCm installed (ld.lld under $ROCM_PATH/llvm/bin, /opt/rocm by default)")]
    GpuSerializationFailed,

    /// A melior call (block argument access, operation result access, ...) failed.
    #[error("melior operation failed: {0}")]
    Melior(#[from] melior::Error),
}
