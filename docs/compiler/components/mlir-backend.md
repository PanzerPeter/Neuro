# MLIR Backend (Experimental)

**Status**: Phase 1.8 scaffold — off by default behind the `mlir` cargo feature
**Crate**: `compiler/mlir-backend`
**Library**: melior 0.25.1 (Rust MLIR bindings, LLVM/MLIR 20)

## Overview

The MLIR backend is the future tensor / autodiff / GPU lowering path (Phase 3+). It consumes the
same typed High-Level IR ([`neuro-hir`](hir-lowering.md)) the LLVM backend consumes. As of the
Phase 1.8 scaffold it emits a trivial, verifier-clean MLIR module — one `func.func` *declaration* per
function and `impl` method — proving the HIR → `melior` → verified-MLIR pipeline end-to-end. Real
body lowering (linalg / tensor dialects) is Phase 3+.

## Feature Gate

The path is opt-in behind the off-by-default `mlir` feature
(`mlir = ["dep:melior", "dep:thiserror", "dep:neuro-hir"]`):

- **Disabled (default)**: the crate compiles to an empty placeholder and pulls in no MLIR toolchain
  (nor `neuro-hir`), so `cargo build/test --workspace` works on a stock LLVM 20 install with no MLIR
  on every CI OS.
- **Enabled**: pulls in `melior` + `neuro-hir` and exposes the entry points below. CI provisions MLIR
  only on Linux, where the `--all-features` lint job and a dedicated
  `cargo test -p mlir-backend --features mlir` smoke step exercise the gated code; the Windows/macOS
  legs build the placeholder.

See [Installation → Optional: MLIR Backend](../../getting-started/installation.md#optional-mlir-backend-phase-18)
for the MLIR 20 + libclang 20 toolchain setup.

## Entry Points (feature `mlir`)

```rust
pub fn lower_program(program: &HirProgram) -> Result<String, MlirError>;
pub fn emit_smoke_module() -> Result<String, MlirError>;
```

- `lower_program` — the HIR → MLIR scaffold: registers all dialects, walks the typed HIR, and returns
  the textual form of a **verified** module of `func.func` declarations.
- `emit_smoke_module` — the HIR-independent `melior` wiring check: builds + verifies
  `func.func @neuro_smoke(index, index) -> index` with an `arith.addi` body.

## Lowering Rules (scaffold)

- Free functions and `impl` methods become `func.func` *declarations* (empty region, private
  visibility — external symbols, not definitions). A method receiver lowers to a pointer parameter.
  Structs and constants are skipped.
- HIR scalar types map to MLIR scalars: `i8`–`i64`, `i1` for `bool`, `i32` for `char`,
  `f16` / `bf16` / `f32` / `f64`.
- Every aggregate / reference / string type maps to an opaque `!llvm.ptr` until real tensor and
  struct lowering lands (Phase 3+).
- `void` is the empty result list in return position; anywhere else it is a
  `MlirError::UnsupportedType`.
- Function bodies are intentionally **not** lowered yet — that is the Phase 3 linalg/tensor work. The
  module is run through the MLIR verifier before its textual form is returned.

## Coexistence with inkwell

`mlir-sys` carries no `llvm-sys` dependency and links its own `MLIR` key, so it coexists with
inkwell's `llvm-20` link without a Cargo `links` conflict. Pointing `MLIR_SYS_200_PREFIX` /
`TABLEGEN_200_PREFIX` at the same LLVM 20 build as `LLVM_SYS_201_PREFIX` makes both bindings share one
`libLLVM-20` dylib. That prefix must include MLIR (`mlir-c` headers + `libMLIR*`); Arch's stock
`llvm20` omits MLIR, so build LLVM 20 with `-DLLVM_ENABLE_PROJECTS=mlir`.

`melior 0.25.1` is the newest release targeting MLIR 20 (via `mlir-sys 0.5.0`); `melior 0.26+` moved
to MLIR 21/22.

## Resources

- [mlir-backend CONTEXT](../../../compiler/mlir-backend/CONTEXT.md) — slice contract
- [melior](https://github.com/raviqqe/melior) — Rust MLIR bindings
- [MLIR](https://mlir.llvm.org/) — Multi-Level Intermediate Representation
