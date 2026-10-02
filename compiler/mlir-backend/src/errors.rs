use shared_types::Span;
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

    /// A GPU chip LLVM has no processor model for. LLVM would ignore it with a warning,
    /// and crash selecting AMD instructions; it is also spliced into a textual pass
    /// pipeline, so only a name on the list may reach it.
    #[error(
        "unknown GPU chip `{0}`: expected a processor LLVM 23 knows, such as `sm_80` or `gfx90a`"
    )]
    InvalidGpuChip(String),

    /// `gpu-module-to-binary` produced no device object for a kernel module. An AMD
    /// code object is linked by ROCm's `ld.lld`, found under `$ROCM_PATH/llvm/bin`.
    #[error(
        "GPU kernels could not be serialized; an AMD target needs ROCm installed (ld.lld under $ROCM_PATH/llvm/bin, /opt/rocm by default)"
    )]
    GpuSerializationFailed,

    /// `@gpu` functions whose bodies this path cannot turn into kernels, each with its
    /// declaration's span. Running one on the host instead is what `@gpu` forbids.
    #[error("no GPU kernel for `@gpu` {}: the GPU path lowers straight-line tensor code over `f32` / `f64` or integer tensors of static shape and rank 1 or more (operators, `@`, reductions, sorts, elementwise math, slices at literal positions, permutations, `einsum`), with a tensor result; elementwise math also needs the GPU vendor's device math library (libdevice from the CUDA toolkit, or ROCm's)", names(.0))]
    GpuBodiesNotLowered(Vec<(String, Span)>),

    /// `@kernel` functions whose bodies this path cannot lower, each with the construct
    /// it stopped at: its span and what it is. A kernel has no host body to fall back on.
    #[error("no GPU kernel for `@kernel` {}", kernel_names(.0))]
    KernelBodiesNotLowered(Vec<KernelRefusal>),

    /// A melior call (block argument access, operation result access, ...) failed.
    #[error("melior operation failed: {0}")]
    Melior(#[from] melior::Error),
}

/// Where a `@kernel` body stopped lowering: the function, the construct's span, and what
/// the construct is, completing "cannot lower ...".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KernelRefusal {
    pub function: String,
    pub span: Span,
    pub what: String,
}

fn kernel_names(refusals: &[KernelRefusal]) -> String {
    let quoted: Vec<String> = refusals
        .iter()
        .map(|refusal| format!("'{}' ({})", refusal.function, refusal.what))
        .collect();
    quoted.join(", ")
}

fn names(functions: &[(String, Span)]) -> String {
    let quoted: Vec<String> = functions
        .iter()
        .map(|(name, _)| format!("'{name}'"))
        .collect();
    quoted.join(", ")
}
