//! MLIR backend for Neuro's tensor / autodiff / GPU lowering path.
//!
//! This slice owns the `melior` (Rust MLIR bindings) integration. It lowers the
//! typed HIR to MLIR, turning element-wise tensor arithmetic into `linalg` and
//! leaving every other function an external declaration, because scalar codegen
//! belongs to the LLVM backend alone and must not exist twice.
//!
//! The MLIR path is gated behind the off-by-default `mlir` feature so the
//! workspace still builds and tests on a stock LLVM 20 install without an MLIR
//! toolchain. With the feature disabled this crate is an empty placeholder; with
//! it enabled it pulls in `melior` and exposes `lower_program` (HIR → MLIR),
//! `translate_to_llvm_ir` (that module carried on through the `llvm` dialect into
//! an inkwell LLVM module), plus `emit_smoke_module` (the pure-`melior` wiring
//! check).

#[cfg(feature = "mlir")]
mod bridge;
#[cfg(feature = "mlir")]
mod context;
#[cfg(feature = "mlir")]
mod errors;
#[cfg(feature = "mlir")]
mod lower;
#[cfg(feature = "mlir")]
mod smoke;
#[cfg(feature = "mlir")]
mod tensor_arithmetic;

#[cfg(feature = "mlir")]
pub use bridge::translate_to_llvm_ir;
#[cfg(feature = "mlir")]
pub use errors::MlirError;
#[cfg(feature = "mlir")]
pub use lower::lower_program;
#[cfg(feature = "mlir")]
pub use smoke::emit_smoke_module;
