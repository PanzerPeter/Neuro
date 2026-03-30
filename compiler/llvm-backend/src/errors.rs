// NEURO Programming Language - LLVM Backend
// Code generation error definitions

use thiserror::Error;

/// Code generation errors with detailed context
#[derive(Debug, Error)]
pub enum CodegenError {
    #[error("failed to initialize LLVM context: {0}")]
    InitializationFailed(String),

    #[error("unsupported type: {0}")]
    UnsupportedType(String),

    #[error("undefined variable: {0}")]
    UndefinedVariable(String),

    #[error("undefined function: {0}")]
    UndefinedFunction(String),

    #[error("type mismatch: expected {expected}, found {found}")]
    TypeMismatch { expected: String, found: String },

    #[error("invalid operand type for operator {op}: {ty}")]
    InvalidOperandType { op: String, ty: String },

    #[error("LLVM error: {0}")]
    LlvmError(String),

    #[error("missing return statement in non-void function")]
    MissingReturn,

    #[error("internal compiler error: {0}")]
    InternalError(String),

    #[error("invalid optimization level: {0} (expected 0..=3)")]
    InvalidOptimizationLevel(u8),
}

/// Result type for code generation operations
pub type CodegenResult<T> = Result<T, CodegenError>;
