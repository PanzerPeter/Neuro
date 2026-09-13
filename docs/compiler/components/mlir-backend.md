# MLIR Backend (Experimental)

**Status**: scaffold, off by default behind the `mlir` cargo feature
**Crate**: `compiler/mlir-backend`
**Library**: melior 0.25.1 (Rust MLIR bindings, LLVM/MLIR 20)

## Overview

The MLIR backend is the future tensor / autodiff / GPU lowering path (Phase 2+). It consumes the
same typed High-Level IR ([`neuro-hir`](hir-lowering.md)) the LLVM backend consumes. The scaffold
emits a trivial, verifier-clean MLIR module, one `func.func` *declaration* per function and `impl`
method, and can carry that module on through the `llvm` dialect into a verified inkwell LLVM
module, proving the HIR → MLIR → llvm dialect → inkwell pipeline end-to-end. Real body lowering
(linalg / tensor dialects) is the rest of Phase 2.

## Feature Gate

The path is opt-in behind the off-by-default `mlir` feature
(`mlir = ["dep:melior", "dep:mlir-sys", "dep:inkwell", "dep:thiserror", "dep:neuro-hir"]`):

- **Disabled (default)**: the crate compiles to an empty placeholder and pulls in no MLIR toolchain
  (nor `neuro-hir`), so `cargo build/test --workspace` works on a stock LLVM 20 install with no MLIR
  on every CI OS.
- **Enabled**: pulls in `melior` + `mlir-sys` + `inkwell` + `neuro-hir` and exposes the entry points
  below. CI provisions MLIR only on Linux, where the `--all-features` lint job and a dedicated
  `cargo test -p mlir-backend --features mlir` smoke step exercise the gated code; the Windows/macOS
  legs build the placeholder.

See [Installation → Optional: MLIR Backend](../../getting-started/installation.md#optional-mlir-backend)
for the MLIR 20 + libclang 20 toolchain setup.

## Entry Points (feature `mlir`)

```rust
pub fn lower_program(program: &HirProgram) -> Result<String, MlirError>;
pub fn translate_to_llvm_ir(program: &HirProgram) -> Result<String, MlirError>;
pub fn emit_smoke_module() -> Result<String, MlirError>;
```

- `lower_program`, the HIR → MLIR scaffold: registers all dialects, walks the typed HIR, and returns
  the textual form of a **verified** module of `func.func` declarations.
- `translate_to_llvm_ir`, the full path: the same module, converted to the `llvm` dialect, translated
  into an inkwell LLVM module, LLVM-verified, and returned as textual LLVM IR.
- `emit_smoke_module`, the HIR-independent `melior` wiring check: builds + verifies
  `func.func @neuro_smoke(index, index) -> index` with an `arith.addi` body.

## Lowering Rules (scaffold)

- Free functions and `impl` methods become `func.func` *declarations* (empty region, private
  visibility, external symbols, not definitions). A method receiver lowers to a pointer parameter.
  Structs and constants are skipped.
- HIR scalar types map to MLIR scalars: `i8` to `i64`, `i1` for `bool`, `i32` for `char`,
  `f16` / `bf16` / `f32` / `f64`.
- Every aggregate / reference / string type maps to an opaque `!llvm.ptr` until real tensor and
  struct lowering lands (Phase 2+).
- `void` is the empty result list in return position; anywhere else it is a
  `MlirError::UnsupportedType`.
- Function bodies are intentionally **not** lowered yet; that is the Phase 2 linalg/tensor work. The
  module is run through the MLIR verifier before its textual form is returned.

## MLIR to LLVM IR

`translate_to_llvm_ir` runs a real MLIR conversion pipeline rather than emitting LLVM by hand:

1. `func-to-llvm`, `arith-to-llvm`, `index-to-llvm` rewrite the module into the `llvm` dialect.
2. `reconcile-unrealized-casts` clears the `unrealized_conversion_cast` ops each conversion leaves
   at its boundary with the dialects the others own. The translation rejects any that survive, so
   this pass runs last by necessity, not by convention.
3. `mlirTranslateModuleToLLVMIR` builds the LLVM module. `melior 0.25` does not wrap it, so the
   call goes through `mlir-sys` directly, pinned to the exact version melior itself depends on so
   both reach one crate instance.
4. The resulting `LLVMModuleRef` is wrapped by `inkwell::module::Module` (sole owner, disposed on
   drop) and put through LLVM's verifier.

The `LLVMContext` in step 3 is **inkwell's own**. That is deliberate: `mlir-sys` and `llvm-sys` are
independent bindings, and an install where they resolve to different `libLLVM-20` copies fails at
this handoff instead of miscompiling downstream. Errors are values throughout, one variant per
stage: `PassPipelineFailed`, `TranslationFailed`, `LlvmVerificationFailed`.

## Coexistence with inkwell

`mlir-sys` carries no `llvm-sys` dependency and links its own `MLIR` key, so it coexists with
inkwell's `llvm-20` link without a Cargo `links` conflict. Pointing `MLIR_SYS_200_PREFIX` /
`TABLEGEN_200_PREFIX` at the same LLVM 20 build as `LLVM_SYS_201_PREFIX` makes both bindings share one
`libLLVM-20` dylib. That prefix must include MLIR (`mlir-c` headers + `libMLIR*`); Arch's stock
`llvm20` omits MLIR, so build LLVM 20 with `-DLLVM_ENABLE_PROJECTS=mlir`.

`melior 0.25.1` is the newest release targeting MLIR 20 (via `mlir-sys 0.5.0`); `melior 0.26+` moved
to MLIR 21/22.

## Resources

- [mlir-backend CONTEXT](../../../compiler/mlir-backend/CONTEXT.md), slice contract
- [melior](https://github.com/raviqqe/melior), Rust MLIR bindings
- [MLIR](https://mlir.llvm.org/), Multi-Level Intermediate Representation
