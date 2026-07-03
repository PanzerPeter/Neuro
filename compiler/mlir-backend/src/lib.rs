//! MLIR backend plumbing for Neuro (Phase 1.8).
//!
//! This slice owns the `melior` (Rust MLIR bindings) integration that the
//! tensor / autodiff / GPU lowering path depends on from Phase 3 onward. At this
//! stage it does not yet consume HIR or participate in compilation; it exists to
//! anchor the `melior` dependency alongside inkwell and prove that both bindings
//! link against the same LLVM 20 toolchain. The HIR-consuming lowering entry
//! point lands once the typed HIR is in place.
//!
//! The MLIR path is gated behind the off-by-default `mlir` feature so the
//! workspace still builds and tests on a stock LLVM 20 install without an MLIR
//! toolchain. With the feature disabled this crate is an empty placeholder; with
//! it enabled it pulls in `melior` and exposes `lower_program` (the HIR → MLIR
//! scaffold) plus `emit_smoke_module` (the pure-`melior` wiring check).

#[cfg(feature = "mlir")]
mod errors;
#[cfg(feature = "mlir")]
mod lower;
#[cfg(feature = "mlir")]
mod smoke;

#[cfg(feature = "mlir")]
pub use errors::MlirError;
#[cfg(feature = "mlir")]
pub use lower::lower_program;
#[cfg(feature = "mlir")]
pub use smoke::emit_smoke_module;
