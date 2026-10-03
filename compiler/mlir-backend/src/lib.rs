//! MLIR backend for Neuro's tensor / autodiff / GPU lowering path.
//!
//! This slice owns the `melior` (Rust MLIR bindings) integration. It lowers the
//! typed HIR to MLIR, turning element-wise tensor arithmetic into `linalg` and
//! leaving every other function an external declaration, because scalar codegen
//! belongs to the LLVM backend alone and must not exist twice.
//!
//! It exposes `lower_program` (HIR → MLIR), `translate_to_llvm_ir` (that module
//! carried on through the `llvm` dialect into an inkwell LLVM module),
//! `lower_for_link` (the bodies the driver links into the LLVM backend's module)
//! and `lower_for_gpu` (those bodies as NVIDIA or AMD kernels behind host
//! launchers).

mod bridge;
mod context;
mod errors;
mod gpu;
mod guards;
mod kernel;
mod lower;
#[cfg(test)]
mod smoke;
mod tensor_apply;
mod tensor_arithmetic;
mod tensor_compound;
mod tensor_einsum;
mod tensor_layout;
mod tensor_math;
mod tensor_reduce;
mod tensor_sort;

pub use bridge::{LinkableBodies, lower_for_link, translate_to_llvm_ir};
pub use errors::{KernelRefusal, MlirError};
pub use gpu::{GpuTarget, lower_for_gpu};
pub use guards::{Guard, GuardKind, Overflow};
pub use lower::lower_program;
