# MLIR Backend (Experimental)

**Status**: experimental, off by default behind the `mlir` cargo feature
**Crate**: `compiler/mlir-backend`
**Library**: melior 0.25.1 (Rust MLIR bindings, LLVM/MLIR 20)

## Overview

The MLIR backend is the tensor / autodiff / GPU lowering path. It consumes the same typed
High-Level IR ([`neuro-hir`](hir-lowering.md)) the LLVM backend consumes and emits a verifier-clean
MLIR module: one `func.func` *declaration* per function and `impl` method, except where a body is
element-wise tensor arithmetic, which becomes a definition built from the `linalg` and `tensor`
dialects. That module can be carried on through the `llvm` dialect into a verified inkwell LLVM
module, proving the HIR → MLIR → llvm dialect → inkwell pipeline end-to-end.

Scalar arithmetic is deliberately **not** lowered here and never will be: it belongs to the
[LLVM backend](llvm-backend.md) alone, so that tensor codegen does not exist in two maintained
copies.

## Feature Gate

The path is opt-in behind the off-by-default `mlir` feature
(`mlir = ["dep:melior", "dep:mlir-sys", "dep:inkwell", "dep:thiserror", "dep:neuro-hir", "dep:ast-types"]`):

The gate is permanent, not a staging step. The only Windows LLVM 20 build shipping the headers and
import libraries `llvm-sys` needs carries no MLIR at all, so requiring MLIR would stop `neurc.exe`
being buildable; Homebrew's `llvm@20` does carry it, and Arch's `llvm20` does not.

- **Disabled (default)**: the crate compiles to an empty placeholder and pulls in no MLIR toolchain
  (nor `neuro-hir`), so `cargo build/test --workspace` works on a stock LLVM 20 install with no MLIR
  on every CI OS.
- **Enabled**: pulls in `melior` + `mlir-sys` + `inkwell` + `neuro-hir` + `ast-types` and exposes the
  entry points below. CI provisions MLIR only on Linux, where the `--all-features` lint job and a dedicated
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

- `lower_program`, the HIR → MLIR lowering: registers all dialects, walks the typed HIR, and returns
  the textual form of a **verified** module.
- `translate_to_llvm_ir`, the full path: the same module, converted to the `llvm` dialect, translated
  into an inkwell LLVM module, LLVM-verified, and returned as textual LLVM IR.
- `emit_smoke_module`, the HIR-independent `melior` wiring check: builds + verifies
  `func.func @neuro_smoke(index, index) -> index` with an `arith.addi` body.

## Lowering Rules

- Free functions and `impl` methods become `func.func` *declarations* (empty region, private
  visibility, external symbols, not definitions). A method receiver lowers to a pointer parameter.
  Structs and constants are skipped.
- HIR scalar types map to MLIR scalars: `i8` to `i64`, `i1` for `bool`, `i32` for `char`,
  `f16` / `bf16` / `f32` / `f64`.
- A tensor maps to a ranked MLIR tensor: `Tensor<f32, [2, 3]>` is `tensor<2x3xf32>`, and a dynamic
  `?` axis becomes MLIR's dynamic-size sentinel. The element must map to an MLIR integer or float;
  an aggregate element is a `MlirError::UnsupportedType`.
- Every other aggregate / reference / string type maps to an opaque `!llvm.ptr` until real struct
  lowering lands.
- `void` is the empty result list in return position; anywhere else it is a
  `MlirError::UnsupportedType`.
- The module is run through the MLIR verifier before its textual form is returned.

## Tensor Arithmetic

A function whose body is a run of `val` bindings closed by one `return`, over element-wise
`+ - * /` on tensors, is emitted as a definition instead. Each operator becomes a `tensor.empty`
destination plus one `linalg.generic`:

```mlir
#map = affine_map<(d0, d1) -> (d0, d1)>
func.func @f(%arg0: tensor<2x3xf32>, %arg1: tensor<2x3xf32>) -> tensor<2x3xf32> {
  %0 = tensor.empty() : tensor<2x3xf32>
  %1 = linalg.generic {indexing_maps = [#map, #map, #map],
                       iterator_types = ["parallel", "parallel"]}
       ins(%arg0, %arg1 : tensor<2x3xf32>, tensor<2x3xf32>)
       outs(%0 : tensor<2x3xf32>) {
  ^bb0(%in: f32, %in_0: f32, %out: f32):
    %2 = arith.addf %in, %in_0 : f32
    linalg.yield %2 : f32
  } -> tensor<2x3xf32>
  return %1 : tensor<2x3xf32>
}
```

Float elements use the `arith` float operations and integer elements theirs, with division
splitting on signedness (`divsi` / `divui`).

### Broadcasting

Operands need not share the result's shape. Each one gets its own indexing map, computed from
its shape against the result's, and it is that map alone which broadcasts:

| Operand | Map, for a `[2, 3]` result | Meaning |
| --- | --- | --- |
| `Tensor<f32, [2, 3]>` | `(d0, d1) -> (d0, d1)` | walks every axis |
| `Tensor<f32, [1, 3]>` | `(d0, d1) -> (0, d1)` | the size-1 axis is stretched: read at index 0 |
| `Tensor<f32, [3]>` | `(d0, d1) -> (d1)` | lower rank, aligned at the trailing axis |
| `f32` | `(d0, d1) -> ()` | a scalar, read at every point |

Shapes align at the **trailing** axis, so a lower-rank operand supplies the innermost axes and
repeats across the leading ones. The destination always keeps the identity map, which is what
makes the operation element-wise rather than a gather.

A dynamic `?` extent lowers too: `tensor.empty` takes one size operand per dynamic axis, read
back with `tensor.dim` on an operand that walks that axis. A stretched operand cannot supply
one, since it is size 1 there and says nothing about the result.

Anything the builder cannot express leaves the function an external declaration rather than
failing: scalar bodies and every other tensor operation, which stay on the LLVM backend by
design; an extent that neither matches the result's nor is 1; an operand outranking the result;
a different element type; a `?` extent no operand can prove equal to the result's, which is
never stretched because nothing at compile time can show it is 1; a literal operand; and
`f16` / `bf16` elements.

A `linalg` body does **not** survive `translate_to_llvm_ir`: the pipeline below covers
`func` / `arith` / `index` only, and bufferizing `linalg` is later work. The crossing returns a
typed error rather than a wrong module.

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
