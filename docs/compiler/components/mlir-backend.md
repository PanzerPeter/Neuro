# MLIR Backend (Experimental)

**Status**: experimental, off by default behind the `mlir` cargo feature
**Crate**: `compiler/mlir-backend`
**Library**: melior 0.27.8 (Rust MLIR bindings, LLVM/MLIR 22)

## Overview

The MLIR backend is the tensor lowering path that GPU dialects will extend later (see the
[Quick Roadmap](../../../README.md#quick-roadmap)). It consumes the same typed High-Level IR ([`neuro-hir`](hir-lowering.md)) the LLVM backend consumes and emits a verifier-clean
MLIR module: one `func.func` *declaration* per function and `impl` method, except where a body is
element-wise tensor arithmetic or a matrix product, which becomes a definition built from the
`linalg` and `tensor` dialects. That module can be carried on through bufferization and the
`llvm` dialect into a verified inkwell LLVM module, where a `linalg` body arrives as a real loop nest, proving the
HIR → MLIR → llvm dialect → inkwell pipeline end-to-end. A `neurc` built with its own `mlir`
feature links the bodies this path computes into every program it compiles; see
[Linking into a compile](#linking-into-a-compile).

Scalar arithmetic is deliberately **not** lowered here and never will be: it belongs to the
[LLVM backend](llvm-backend.md) alone, so that tensor codegen does not exist in two maintained
copies.

## Architecture

- **Dependencies** (all behind the `mlir` feature): `neuro-hir` (the HIR it lowers), `ast-types`,
  `melior`, `mlir-sys`, `inkwell`, `thiserror`. It depends on no feature slice.
- **Public API** (feature `mlir`): `lower_program`, `translate_to_llvm_ir`, `lower_for_link`,
  `LinkableBodies`, `MlirError`.
- **Reached from `neurc`** through `lower_for_link`, when `neurc` is built with its `mlir` feature.

### Feature gate

The path is opt-in behind the off-by-default `mlir` feature
(`mlir = ["dep:melior", "dep:mlir-sys", "dep:inkwell", "dep:thiserror", "dep:neuro-hir", "dep:ast-types"]`):

The gate is permanent, not a staging step. LLVM's official Windows development build, the one
`llvm-sys` builds against there, carries no MLIR at all, so requiring MLIR would stop `neurc.exe`
being buildable. Homebrew's `llvm@22` and apt.llvm.org's packages do carry it; on Arch no
package fits (`aur/mlir` ships no `libMLIR-C.so`), so MLIR comes from a source build.

- **Disabled (default)**: the crate compiles to an empty placeholder and pulls in no MLIR toolchain
  (nor `neuro-hir`), so `cargo build/test --workspace` works on a stock LLVM 22 install with no MLIR
  on every CI OS.
- **Enabled**: pulls in `melior` + `mlir-sys` + `inkwell` + `neuro-hir` + `ast-types` and exposes the
  entry points below. CI provisions MLIR only on Linux, where the `--all-features` lint job and a dedicated
  `cargo test -p mlir-backend --features mlir` smoke step exercise the gated code; the Windows/macOS
  legs build the placeholder.

See [Installation → Optional: MLIR Backend](../../getting-started/installation.md#optional-mlir-backend)
for the MLIR 22 toolchain setup.

### Entry points (feature `mlir`)

```rust
pub fn lower_program(program: &HirProgram) -> Result<String, MlirError>;
pub fn translate_to_llvm_ir(program: &HirProgram) -> Result<String, MlirError>;
pub fn lower_for_link(program: &HirProgram) -> Result<LinkableBodies, MlirError>;
```

- `lower_program`, the HIR → MLIR lowering: registers all dialects, walks the typed HIR, and returns
  the textual form of a **verified** module.
- `translate_to_llvm_ir`, the full path: the same module, converted to the `llvm` dialect, translated
  into an inkwell LLVM module, LLVM-verified, and returned as textual LLVM IR.
- `lower_for_link`, the driver's entry: only the bodies worth linking, as LLVM IR, with the
  function each symbol computes.
- The HIR-independent `melior` wiring check (a verified `func.func @neuro_smoke` with an
  `arith.addi` body) is `pub(crate)` and compiled only under `test`.

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

A function whose body is a run of `val` bindings closed by one `return` or a tail expression,
over element-wise `+ - * /` on tensors, is emitted as a definition instead. An operand may be
owned or borrowed: a `&Tensor` parameter, or `&a` in the expression, reads the same elements. Each operator becomes a `tensor.empty`
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
never stretched because nothing at compile time can show it is 1; a literal operand; a body that
hands back an argument unchanged; and `f16` / `bf16` elements.

### Matrix multiplication

`@` takes a path of its own: a matrix product contracts an axis rather than walking one, so its
index space has a third dimension no operand of the result has. It emits three operations: a
`tensor.empty`, a `linalg.generic` that fills it with the element's zero, and a second one that
accumulates into the filled destination:

```mlir
#zero = affine_map<(d0, d1) -> ()>
#out  = affine_map<(d0, d1) -> (d0, d1)>
#lhs  = affine_map<(d0, d1, d2) -> (d0, d2)>
#rhs  = affine_map<(d0, d1, d2) -> (d2, d1)>
#acc  = affine_map<(d0, d1, d2) -> (d0, d1)>
func.func @f(%arg0: tensor<2x3xf32>, %arg1: tensor<3x4xf32>) -> tensor<2x4xf32> {
  %0 = tensor.empty() : tensor<2x4xf32>
  %cst = arith.constant 0.000000e+00 : f32
  %1 = linalg.generic {indexing_maps = [#zero, #out],
                       iterator_types = ["parallel", "parallel"]}
       ins(%cst : f32) outs(%0 : tensor<2x4xf32>) {
  ^bb0(%in: f32, %out: f32):
    linalg.yield %in : f32
  } -> tensor<2x4xf32>
  %2 = linalg.generic {indexing_maps = [#lhs, #rhs, #acc],
                       iterator_types = ["parallel", "parallel", "reduction"]}
       ins(%arg0, %arg1 : tensor<2x3xf32>, tensor<3x4xf32>)
       outs(%1 : tensor<2x4xf32>) {
  ^bb0(%in: f32, %in_0: f32, %out: f32):
    %3 = arith.mulf %in, %in_0 : f32
    %4 = arith.addf %out, %3 : f32
    linalg.yield %4 : f32
  } -> tensor<2x4xf32>
  return %2 : tensor<2x4xf32>
}
```

`(d0, d1, d2)` is (row, column, contracted), and the reduction iterator comes last because that
is the order the maps number the dimensions in. The fill is not optional: a reduction reads its
destination at every point, which is what makes it an accumulator, and `tensor.empty` is
undefined memory.

Named `linalg.matmul` and `linalg.fill` are not reachable (melior's ODS module generates from
`LinalgOps.td` only) so all three generics go through one builder that takes its operand split,
maps, iterators and body region as arguments. Every extent must be static: `tensor.dim` can
recover a dynamic result axis but not the contracted one, which appears in no operand of the
destination, so a `?` anywhere leaves the function a declaration.

A `linalg` body survives `translate_to_llvm_ir`: the pipeline below bufferizes it and turns it
into loops. See [MLIR to LLVM IR](#mlir-to-llvm-ir).

## MLIR to LLVM IR

`translate_to_llvm_ir` runs a real MLIR pass pipeline rather than emitting LLVM by hand:

1. `one-shot-bufferize{bufferize-function-boundaries=true function-boundary-type-conversion=identity-layout-map}`
   rewrites tensor values into `memref` buffers. The function-boundary flag is required, not a
   tuning knob: without it a `func.func` keeps `tensor` in its signature, which `func-to-llvm`
   cannot convert. The identity layout makes a parameter a plain row-major `memref`, which is
   what a DLPack buffer is.
2. `buffer-results-to-out-params{hoist-static-allocs=true modify-public-functions=true}` turns a
   returned buffer into a trailing parameter the caller allocates, and with a static shape the
   body writes straight into it.
3. `buffer-deallocation-pipeline` gives each buffer still allocated inside an owner and a release.
4. `func.func(convert-linalg-to-loops)` turns the structured op into `scf` loops over element
   loads and stores. It is nested under `func.func` because that is the operation it is anchored
   on, and it must run *after* bufferization: against tensor operands it silently leaves the op
   alone.
5. `convert-scf-to-cf` and `finalize-memref-to-llvm` lower what those loops are made of, then
   `func-to-llvm`, `arith-to-llvm`, `cf-to-llvm` and `index-to-llvm` take the rest into the
   `llvm` dialect.
6. `reconcile-unrealized-casts` clears the `unrealized_conversion_cast` ops each conversion leaves
   at its boundary with the dialects the others own. The translation rejects any that survive, so
   this pass runs last by necessity, not by convention.
7. `mlirTranslateModuleToLLVMIR` builds the LLVM module. melior does not wrap it, so the
   call goes through `mlir-sys` directly, pinned to the exact version melior itself depends on so
   both reach one crate instance.
8. The resulting `LLVMModuleRef` is wrapped by `inkwell::module::Module` (sole owner, disposed on
   drop) and put through LLVM's verifier.

The pipeline is named in text and parsed with `melior::utility::parse_pass_pipeline`, because
two of the three entries that carry a `linalg` body have no usable typed constructor: melior's
`one-shot-bufferize` takes no options, so it cannot set `bufferize-function-boundaries`, and
`buffer-deallocation-pipeline` is a pipeline rather than a pass. Textually named passes must be in the process-global pass registry, so the
MLIR context builder calls `register_all_passes` once.

A bufferized tensor parameter crosses as an exploded `memref` descriptor (allocated pointer,
aligned pointer, offset, sizes, strides), and so does the result, as the last parameter. That is
MLIR's tensor ABI, not the single DLPack handle the LLVM backend uses; the section below is where
the two meet.

The `LLVMContext` in step 7 is **inkwell's own**. That is deliberate: `mlir-sys` and `llvm-sys` are
independent bindings, and an install where they resolve to different `libLLVM` copies fails at
this handoff instead of miscompiling downstream. Errors are values throughout, one variant per
stage: `PassPipelineFailed`, `TranslationFailed`, `LlvmVerificationFailed`.

## Linking into a compile

`lower_for_link` builds a second module holding only the bodies that are safe to swap in, each
defined under `__neuro_mlir_` plus its function's name, and returns its LLVM IR with the
`(function, symbol)` pairs. A body qualifies when it lowers (above) and its signature is `f32` /
`f64` scalars and static tensors of them, owned or behind `&`, returning a static tensor.

Integer elements stay on the LLVM backend because it guards them (an overflowing element panics
in a debug build, a zero divisor in every build) and `arith` has neither guard. A dynamic
signature never has a body to link, since the frontend gives a `?` axis no arithmetic. The module
declares nothing, because a declaration here would name a Neuro-ABI function at an MLIR
signature.

The [LLVM backend](llvm-backend.md#mlir-bodies) defines each such function as a call to its
symbol and links the IR in. The result buffer is the LLVM backend's allocation, so it is a
DLPack tensor like any other; a buffer the body needs in between is allocated and freed inside
it.

## Coexistence with inkwell

`mlir-sys` carries no `llvm-sys` dependency and no Cargo `links` key that clashes with inkwell's.
It finds MLIR by running `$MLIR_SYS_220_PREFIX/bin/llvm-config`, so MLIR must be installed into
the same prefix as the LLVM `LLVM_SYS_221_PREFIX` names, and `TABLEGEN_220_PREFIX` names it too.
Both bindings then load one `libLLVM` 22, which the handoff above relies on.

`melior 0.27.x` is the last line on MLIR 22 (via `mlir-sys 220`); `melior 0.28` moved to MLIR 23.

## Resources

- [mlir-backend CONTEXT](../../../compiler/mlir-backend/CONTEXT.md), slice contract
- [melior](https://github.com/raviqqe/melior), Rust MLIR bindings
- [MLIR](https://mlir.llvm.org/), Multi-Level Intermediate Representation
