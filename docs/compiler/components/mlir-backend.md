# MLIR Backend (Experimental)

**Status**: experimental, built into every compiler
**Crate**: `compiler/mlir-backend`
**Library**: melior 0.28.2 (Rust MLIR bindings, LLVM/MLIR 23)

## Overview

The MLIR backend computes every tensor operation, on the CPU and as NVIDIA or AMD GPU kernels
(see [GPU kernels](#gpu-kernels)). It consumes the same typed High-Level IR
([`neuro-hir`](hir-lowering.md)) the LLVM backend consumes, in which
[lowering](hir-lowering.md#tensor-operations) has outlined each tensor operation into a function
of its own, and builds each one's body from the `linalg`, `tensor` and `memref` dialects. The
module is carried on through bufferization and the `llvm` dialect into a verified inkwell LLVM
module, where a `linalg` body arrives as a real loop nest. `neurc` links these bodies into every
program it compiles; see [Linking into a compile](#linking-into-a-compile).

The two backends split the work by kind. Every loop over a tensor's elements is here, and the
LLVM backend has none of its own; scalar code, and the tensor work with no loop (building a
handle, reading one element, moving a tensor between devices), belong to the
[LLVM backend](llvm-backend.md) alone. Neither is written twice.

## Architecture

- **Dependencies**: `neuro-hir` (the HIR it lowers), `ast-types`, `shared-types`,
  `melior`, `mlir-sys`, `inkwell`, `thiserror`. It depends on no feature slice.
- **Public API**: `lower_program`, `translate_to_llvm_ir`, `lower_for_link`,
  `lower_for_gpu`, `GpuTarget`, `LinkableBodies`, `MlirError`.
- **Reached from `neurc`** through `lower_for_link` and `lower_for_gpu`.

### Toolchain

Every build compiles this crate, so every build needs MLIR 23 in the same prefix as LLVM 23.
apt.llvm.org's packages and Homebrew's `llvm` carry it. On Arch no package fits (`aur/mlir`
ships no `libMLIR-C.so`), and no Windows LLVM release carries MLIR at all, so on both it comes
from a source build. See [Installation → MLIR](../../getting-started/installation.md#mlir).

### Entry points

```rust
pub fn lower_program(program: &HirProgram) -> Result<String, MlirError>;
pub fn translate_to_llvm_ir(program: &HirProgram) -> Result<String, MlirError>;
pub fn lower_for_link(program: &HirProgram) -> Result<LinkableBodies, MlirError>;
pub fn lower_for_gpu(program: &HirProgram, target: &GpuTarget) -> Result<LinkableBodies, MlirError>;
```

- `lower_program`, the HIR → MLIR lowering: registers all dialects, walks the typed HIR, and returns
  the textual form of a **verified** module.
- `translate_to_llvm_ir`, the full path: the same module, converted to the `llvm` dialect, translated
  into an inkwell LLVM module, LLVM-verified, and returned as textual LLVM IR.
- `lower_for_link`, the driver's entry: the host body of every outlined tensor operation and of
  every `@gpu(fallback: true)` function, as LLVM IR, with the function each symbol computes.
- `lower_for_gpu`, the `@gpu` functions (`fallback: true` ones included) and the `@kernel` ones
  as GPU kernels with host functions that launch them, reading and writing device memory only,
  or an error naming each function it cannot lower. It also takes the tensor operations lowering
  outlined to run where their operands live, without refusing any it cannot lower. `neurc` calls
  it for a program with any of them.
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
- A function value maps to `!llvm.struct<(ptr, ptr)>`, the LLVM backend's pair of the function
  and its environment. Every other aggregate / reference / string type maps to an opaque
  `!llvm.ptr`.
- `void` is the empty result list in return position; anywhere else it is a
  `MlirError::UnsupportedType`.
- The module is run through the MLIR verifier before its textual form is returned.

## Tensor Arithmetic

A function whose body is a run of `val` bindings closed by one `return` or a tail expression,
over element-wise `+ - * / %` on tensors and the operations below, is emitted as a definition.
An operand may be owned or borrowed: a `&Tensor` or `&mut Tensor` parameter, or `&a` in the
expression, reads the same elements. Each operator becomes a `tensor.empty` destination plus one
`linalg.generic`:

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

Float elements use the `arith` float operations (`remf` for `%`, which is C's `fmod`). Integer
elements use theirs, with division and remainder splitting on signedness (`divsi` / `divui`,
`remsi` / `remui`), plus the checks described under
[Linking into a compile](#linking-into-a-compile). A half-precision element (`f16`, `bf16`) is
widened to `f32` for each operation and the answer rounded back once, so `(a * b) + c` rounds
twice, as two separate operations do.

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
failing: scalar bodies, which stay on the LLVM backend by design; an extent that neither matches
the result's nor is 1; an operand outranking the result; a different element type; a `?` extent
no operand can prove equal to the result's, which is never stretched because nothing at compile
time can show it is 1; and a body that hands back an argument unchanged. A scalar operand, a
literal included, reaches an outlined body as a parameter. For an outlined operation a
declaration is a compiler bug, which `neurc` reports at the operation.

### The other operations

Every one of these lowers for the host and for a GPU alike.

- **Reductions** (`.sum()`, `.mean()`, `.max()`, `.min()`): a seed and a fold, one result element
  per parallel point folding its run in order, and runs longer than the language's 4096
  reduction lanes folded lane by lane first, as [GPU kernels](#gpu-kernels) describes. A
  half-precision run is widened to `f32`, folded there, and rounded back once.
- **Sorts** (`.sort()`, `.argsort()`, `.topk()`): a stable bottom-up merge sort, one parallel
  `linalg.generic` per doubling of the block width, each point placed by a binary search; on the
  host, an integer run of 256 or more sorts by LSD radix, a byte a pass, instead.
- **Elementwise math**: the `math` dialect op of each function's name, which the CPU pipeline
  turns into the same LLVM intrinsics the LLVM backend calls on a scalar.
- **Slices and permutations**: a gather reading the source at each result position, and an
  input map.
- **`einsum`**: the matrix product's contracting shape, adding in its letters' order.
- **Compound assignment**, on the host only: one `linalg.generic` over `memref`s whose
  destination is the `&mut` target's own buffer, so the update writes in place and nothing is
  copied.
- **Traversals** (`.map`, `.zip`, `.reduce`), on the host only: one `linalg.generic` whose body
  calls the traversal's function through its function value, element by element in row-major
  order.

A GPU runs compound assignments and traversals as per-thread launchers of its own
([GPU kernels](#gpu-kernels)).

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

### The contraction schedule

Lowered as it is, that contraction is the naive loop nest: one output element at a time, the
contracted axis innermost, the right operand read down a column. Before `lower_for_link` and
`lower_for_gpu` bufferize a module, `schedule.rs` runs a transform-dialect script over every
contraction the builders tagged (`@`, and an `einsum` with a contracted letter), through
`mlirTransformApplyNamedSequence`:

- **Host**: the result axes tile into a register block of 4 rows by 64 bytes of columns (16
  `f32` or 8 `f64`), each contracted letter becomes a loop of its own outside the block, an axis
  the block does not divide is peeled into one smaller static block, every block vectorizes, and
  the accumulator is hoisted out of the contracted loops so it stays in registers.
- **GPU**: the block is up to 4 × 4 elements per thread, sized to divide the result axes, as an
  `scf.forall` the parallel-loop mapping turns into threads. A product a single block would cover
  keeps the plain kernel.

Each element still adds its products one at a time, in contracted order, starting from the fill's
zero, with the multiply and the add rounded separately. A block holds one accumulator per element
and never splits an element's sum, so a scheduled product has exactly the bits of the naive nest,
on the host and on a GPU. That rules out tensor-core MMA and a vendor BLAS, which both split the
contracted extent or fuse the multiply into the add.

A half-precision contraction widens its operands to `f32`, contracts there and rounds each result
element once, so it is scheduled as an `f32` one. A body with integer overflow checks (the `-O0`
tier) and an `einsum` operand that repeats a letter are left as the naive nest.

## MLIR to LLVM IR

`translate_to_llvm_ir` runs a real MLIR pass pipeline rather than emitting LLVM by hand:

1. `linalg-fuse-elementwise-ops` folds each operation whose one use is the next into it, so
   `(a + b) * c` becomes one loop nest and the sum is never stored. The fused body runs the same
   operations in the same order, and no multiply and add are contracted without fast-math flags,
   so the results are bit-for-bit the unfused ones.
2. `one-shot-bufferize{bufferize-function-boundaries=true function-boundary-type-conversion=identity-layout-map}`
   rewrites tensor values into `memref` buffers. The function-boundary flag is required, not a
   tuning knob: without it a `func.func` keeps `tensor` in its signature, which `func-to-llvm`
   cannot convert. The identity layout makes a parameter a plain row-major `memref`, which is
   what a DLPack buffer is.
3. `buffer-results-to-out-params{hoist-static-allocs=true modify-public-functions=true}` turns a
   returned buffer into a trailing parameter the caller allocates, and with a static shape the
   body writes straight into it.
4. `buffer-deallocation-pipeline` gives each buffer still allocated inside an owner and a release.
5. `func.func(convert-linalg-to-loops)` turns the structured op into `scf` loops over element
   loads and stores. It is nested under `func.func` because that is the operation it is anchored
   on, and it must run *after* bufferization: against tensor operands it silently leaves the op
   alone.
6. `convert-vector-to-scf`, `expand-strided-metadata` and `lower-affine` take apart a scheduled
   contraction's vector transfers and tile views. Then `convert-scf-to-cf`, `convert-vector-to-llvm`
   and `finalize-memref-to-llvm` lower what the loops are made of, and `func-to-llvm`,
   `arith-to-llvm`, `cf-to-llvm`, `index-to-llvm` and `ub-to-llvm` take the rest into the `llvm`
   dialect.
7. `reconcile-unrealized-casts` clears the `unrealized_conversion_cast` ops each conversion leaves
   at its boundary with the dialects the others own. The translation rejects any that survive, so
   this pass runs last by necessity, not by convention.
8. `mlirTranslateModuleToLLVMIR` builds the LLVM module. melior does not wrap it, so the
   call goes through `mlir-sys` directly, pinned to the exact version melior itself depends on so
   both reach one crate instance.
9. The resulting `LLVMModuleRef` is wrapped by `inkwell::module::Module` (sole owner, disposed on
   drop) and put through LLVM's verifier.

The pipeline is named in text and parsed with `melior::utility::parse_pass_pipeline`, because
two of the entries that carry a `linalg` body have no usable typed constructor: melior's
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

`lower_for_link` builds a second module holding the host body of every outlined operation and
every `@gpu(fallback: true)` function, each defined under `__neuro_mlir_` plus its function's
name, and returns its LLVM IR with the `(function, symbol)` pairs. A body qualifies when it lowers
(above) and its signature is numbers, `bool`s, static tensors of them, owned or behind `&` /
`&mut`, and function values over them, returning a static tensor, a tuple of them, or nothing.
A body with a tensor result marks its pointer parameters `noalias`: it only reads its operands
and writes a buffer its caller has just allocated, which lets LLVM keep an accumulator in a
register once the body is inlined.

The LLVM backend checks integer elements: an overflowing element panics in a debug build (`-O0`),
a zero divisor in every build, each naming the operator. A body here cannot panic, and on a GPU
nothing can stop the program, so a checked body takes one more parameter, an `i64` status word
its caller fills with all ones. A failed check (LLVM's `with.overflow` intrinsics for `+ - *`, a
compare for a divisor) lowers the word with an atomic unsigned min to a key: the operation's
number, then the element's row-major position, then the check's number in the low 12 bits. The
smallest key is the failure the host would have stopped at, whichever GPU thread got there first.
`lower_for_link` returns each symbol's checks with it, and the LLVM backend panics for the one
left in the word after the call, with its own message and location. `neurc` passes the tier down,
so overflow checks exist only in a debug build. A dynamic
signature never has a body to link, since the frontend gives a `?` axis no arithmetic. The module
declares nothing, because a declaration here would name a Neuro-ABI function at an MLIR
signature.

The [LLVM backend](llvm-backend.md#mlir-bodies) emits each call to such a function as a call to
its symbol and links the IR in. The result buffer is the LLVM backend's allocation, so it is a
DLPack tensor like any other; a buffer the body needs in between is allocated and freed inside
it.

## GPU kernels

`lower_for_gpu` takes the module `lower_for_link` builds and lowers each `linalg` op to a GPU
kernel instead of a loop nest. `GpuTarget::Nvidia { chip }` (an `sm_NN`) goes through `nvvm` and
embeds PTX, which the CUDA driver compiles for the GPU it runs on, so compiling needs no CUDA
toolkit. `GpuTarget::Amd { chip }` (a `gfxNNN`) goes through `rocdl` and embeds a code object
for exactly that chip. Building the code object runs ROCm's `ld.lld`, so an AMD target needs ROCm
installed and fails with `GpuSerializationFailed` without it.

The parallel loops are tiled with a bounds guard rather than a clamped extent, so blocks cover
the grid and each block runs a tile of threads. The tiles fit each function's rank: for ranks 1
to 3 the innermost axis goes on thread x, 32 wide, so a warp reads contiguous memory, with 256
threads a block (`256`, `8 × 32`, `1 × 8 × 32`). Rank 4 and up, and a tensor whose outer axis is
too long for grid y or z, which hold 65,535 blocks, keep a 16 × 16 tile over the first two axes
with the outermost axis on grid x. Functions that need different tilings lower as separate
modules. A matrix
product is two kernels, the zero fill and the contraction, whose threads each compute a block of
the result ([the contraction schedule](#the-contraction-schedule)). A reduction (`.sum()`, `.mean()`,
`.max()`, `.min()`) is a seed and a fold, plus a division for
a mean: one thread per result element folds its run in order, which is the LLVM backend's order,
so the device and host answers match exactly. A run longer than the language's 4096 reduction
lanes first folds into a partials tensor, one thread per lane (and per result element), each
looping over its lane's run positions, and the seed and fold then reduce the lanes: the lane
order every backend shares, which puts a whole-tensor `.sum()` on thousands of threads. A GPU body
also lowers elementwise math, slices, permutations and `einsum`, as the host does. A slice
position is a literal, or a parameter the call site has already checked: a GPU body cannot stop
the program. A body over a half-precision or `bool` tensor stays off the GPU: neither has a device
kernel checked against it. A float `%` calls the device math library's exact `fmod`, as the
math functions below do, rather than the hardware's divide-based `frem`.

A math function becomes a call into the GPU vendor's device math library (libdevice on NVIDIA,
ocml on AMD), which the kernels can link only when the CUDA toolkit or ROCm is found while
compiling. `lower_for_gpu` probes for it once, only for a program whose GPU code calls one, and
without it leaves such a body to the host (an outlined operation) or refuses it (`@gpu`,
`@kernel`).

Each symbol keeps `lower_for_link`'s signature, so the
[LLVM backend](llvm-backend.md#mlir-bodies) wrapper serves either path. The two paths split the
program by `HirFunction::target`: `lower_for_link` takes the outlined operations and the
fallback functions' host copies, and `lower_for_gpu` the `@gpu` ones, with or without a fallback,
and the outlined operations of a program that moves a tensor to a device (`FollowsOperands`).
Device symbols are named `__neuro_gpu_` plus the function's name, so a function's host and
device bodies link side by side. A `FollowsOperands` body it cannot lower keeps its host body
alone. A `@gpu` body `lower_for_link` would not take, or one with a rank-0 tensor in it
(a rank-0 operation has no parallel axis to launch over, so it would run on the host against
device buffers), is `GpuBodiesNotLowered`, with the name and span of each: `@gpu` forbids running
it anywhere but a GPU. The symbol's body calls MLIR's GPU runtime ABI (`mgpuModuleLoad` or
`mgpuModuleLoadJIT`, `mgpuLaunchKernel`, the `mgpuStream*` calls), which the IR declares and the
LLVM backend's runtime defines.

Every buffer a symbol is handed has to be device memory; the LLVM backend's wrapper stages them.
A buffer the body needs between two kernels, such as the product in `(a @ b) * c` (a
contraction does not fuse into the operation after it), is allocated
through `_mlir_memref_to_llvm_alloc` and freed through `_mlir_memref_to_llvm_free` rather than
`malloc` / `free` (`finalize-memref-to-llvm{use-generic-functions=true}`). The LLVM backend
defines both as its device allocator.

### `@kernel` functions

A `@kernel` body is the per-thread code itself, so it does not go through `linalg`. The
[`kernel`](../../../compiler/mlir-backend/src/kernel/mod.rs) module writes each one as MLIR text: a
`func.func` whose tensor parameters are `memref`s, a grid sized from the first `&mut` tensor's
extents and the `threads` block shape, and a `gpu.launch` whose region is the body. Locals are
`memref.alloca` slots hoisted to the region's entry and control flow is `cf` branches, so a loop
needs no loop-carried values and `break` or `return` is a plain branch. Every tensor index is
bounds-checked, every integer divisor tested for zero and, in a debug build, every integer
`+ - *` tested for overflow: NVIDIA stops the thread with
`cf.assert` (a device assertion with a message), and AMD, whose `rocdl` lowering has no
`cf.assert`, with a trap. The block size is written as a constant, because `gpu.block_dim`
lowers to a ROCm device-library call on AMD.

The launchers go through a pipeline of their own: outlining, the vendor conversion and the
shared descent to the `llvm` dialect, without the bufferization prefix. Their buffers are the
caller's from the start, and the deallocation pass that follows bufferization refuses a body that
branches in a loop. The two modules are translated to LLVM IR separately and linked into one.
A construct the body lowering does not cover (a function call, a slice, a `match`) is
`KernelBodiesNotLowered`, with the construct's span and what it is.

The same per-thread lowering runs the outlined operations `linalg` cannot express: a compound
assignment, one thread per element writing the target's own buffer, and `.map` / `.zip`, one
thread per element calling the closure inline, with `.reduce` folding in a single thread in the
host's order. Integer arithmetic there reports a failed check through the same status word and
stops the thread; in a `@kernel` it stops the kernel instead, as a bad index does, and an overflow
is checked in a debug build only. A body the lowering refuses keeps its host copy rather than
being an error. See
[`kernel/body/traversal.rs`](../../../compiler/mlir-backend/src/kernel/body/traversal.rs).

## Coexistence with inkwell

`mlir-sys` carries no `llvm-sys` dependency and no Cargo `links` key that clashes with inkwell's.
It finds MLIR by running `$MLIR_SYS_230_PREFIX/bin/llvm-config`, so MLIR must be installed into
the same prefix as the LLVM `LLVM_SYS_231_PREFIX` names, and `TABLEGEN_230_PREFIX` names it too.
Both bindings then load one `libLLVM` 23, which the handoff above relies on.

`melior 0.28.x` is the MLIR 23 line (via `mlir-sys 230`).

## Source

- [`compiler/mlir-backend/src/lib.rs`](../../../compiler/mlir-backend/src/lib.rs)
- [`compiler/mlir-backend/CONTEXT.md`](../../../compiler/mlir-backend/CONTEXT.md)

## See Also

- [melior](https://github.com/raviqqe/melior), Rust MLIR bindings
- [MLIR](https://mlir.llvm.org/), Multi-Level Intermediate Representation
