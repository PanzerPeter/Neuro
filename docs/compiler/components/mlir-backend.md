# MLIR Backend

The MLIR backend computes every tensor operation, on the CPU and as NVIDIA or AMD GPU kernels. It lowers the typed HIR to `linalg`, `tensor` and `memref` IR, carries that through bufferization into LLVM IR, and hands the result to the [LLVM backend](llvm-backend.md) to link into each program. Read it when a change adds or alters a tensor operation, its GPU kernel, or the way the bodies link into a compile.

**Crate**: `compiler/mlir-backend`
**Library**: melior 0.28.2 (Rust MLIR bindings, LLVM and MLIR 23)

## Overview

The MLIR backend consumes the same typed High-Level IR ([`neuro-hir`](hir-lowering.md)) as the LLVM backend. [Lowering](hir-lowering.md#tensor-operations) has already outlined each tensor operation into a function of its own, and this backend builds that function's body. Each body goes through the `linalg` pipeline and comes out as a verified LLVM module, where a `linalg` body becomes a real loop nest.

The two backends split the work by kind. Every loop over a tensor's elements is here, and the LLVM backend has none of its own. Scalar code, and tensor work with no loop (building a handle, reading one element, moving a tensor between devices), belong to the [LLVM backend](llvm-backend.md) alone. Neither is written twice.

## Architecture

- **Dependencies**: `neuro-hir` (the HIR it lowers), `ast-types`, `shared-types`, `melior`, `mlir-sys`, `inkwell`, `thiserror`. It depends on no feature slice. `syntax-parsing` and `hir-lowering` are dev-dependencies, used by the `@kernel` tests to build HIR from source.
- **Public API**: `lower_program`, `translate_to_llvm_ir`, `lower_for_link`, `lower_for_gpu`, `GpuTarget`, `Overflow`, `LinkableBodies`, `Guard`, `GuardKind`, `KernelRefusal`, `MlirError`
- **Reached from `neurc`** through `lower_for_link`, which every compile calls, and `lower_for_gpu`, which runs for a program with GPU code.

### Entry points

```rust
pub fn lower_program(program: &HirProgram) -> Result<String, MlirError>;
pub fn translate_to_llvm_ir(program: &HirProgram) -> Result<String, MlirError>;
pub fn lower_for_link(program: &HirProgram, overflow: Overflow) -> Result<LinkableBodies, MlirError>;
pub fn lower_for_gpu(program: &HirProgram, target: &GpuTarget, overflow: Overflow) -> Result<LinkableBodies, MlirError>;
```

- `lower_program` walks the typed HIR, registers all dialects, and returns the textual form of a verified module. This is the debugging view: each function is a `func.func` declaration, or a definition where the body is one this crate builds.
- `translate_to_llvm_ir` carries the same module through the pipeline in [MLIR to LLVM IR](#mlir-to-llvm-ir), translates it into an inkwell LLVM module, verifies it, and returns the LLVM IR text.
- `lower_for_link` is the driver's entry for host code. It returns the host body of every outlined tensor operation and of every `@gpu(fallback: true)` function, as LLVM IR with the `(function, symbol)` pairs. See [Linking into a compile](#linking-into-a-compile).
- `lower_for_gpu` returns the `@gpu` functions and the `@kernel` functions as GPU kernels with host functions that launch them. It also takes the tensor operations that run where their operands live. See [GPU kernels](#gpu-kernels).
- `Overflow` is the integer tier. `Checked` is the debug tier, where an overflowing element is a failed check. `Wrapping` is the release tier. `neurc` passes `Checked` at `-O0` and `Wrapping` above.
- `GpuTarget` is `Nvidia { chip }` (an `sm_NN`) or `Amd { chip }` (a `gfxNNN`).

### Toolchain

Every build compiles this crate, so every build needs MLIR 23 in the same prefix as LLVM 23. apt.llvm.org's packages and Homebrew's `llvm` carry it. On Arch no package fits (`aur/mlir` ships no `libMLIR-C.so`), and no Windows LLVM release carries MLIR, so on both it comes from a source build. See [Installation → MLIR](../../getting-started/installation.md#mlir).

### Coexistence with inkwell

`mlir-sys` carries no `llvm-sys` dependency and no Cargo `links` key that clashes with inkwell's. It finds MLIR by running `$MLIR_SYS_230_PREFIX/bin/llvm-config`, so MLIR must be installed into the same prefix as the LLVM that `LLVM_SYS_231_PREFIX` names, and `TABLEGEN_230_PREFIX` names it too. Both bindings then load one `libLLVM` 23, which the handoff in [MLIR to LLVM IR](#mlir-to-llvm-ir) relies on. `melior 0.28.x` is the MLIR 23 line, via `mlir-sys 230`.

## Behavior

### Lowering rules

- Free functions, `impl` methods and lifted closures become `func.func` *declarations* (empty region, private visibility, external symbols). A closure's captured-environment pointer is an implicit first parameter, as in the LLVM backend's calling convention. A method receiver lowers to a pointer parameter. Structs, enums and constants carry no callable surface and are skipped.
- HIR scalars map to MLIR scalars: `i8` to `i64`, `i1` for `bool`, `i32` for `char`, and `f16`, `bf16`, `f32`, `f64`.
- A tensor maps to a ranked MLIR tensor: `Tensor<f32, [2, 3]>` is `tensor<2x3xf32>`, and a dynamic `?` axis becomes MLIR's dynamic-size sentinel. The element must map to an MLIR integer or float. An aggregate element is `MlirError::UnsupportedType`.
- A function value maps to `!llvm.struct<(ptr, ptr)>`, the same function and environment pair the LLVM backend uses. Every other aggregate, reference or string type (tuples, enums, the standard collections, and references to them) maps to an opaque `!llvm.ptr`. A reference to a tensor is the tensor itself. A newtype maps to its inner type.
- `void` is the empty result list in return position. Anywhere else it is `MlirError::UnsupportedType`. The unsized types (`dyn Trait`, `[T]`) reach a value position only behind the reference that already maps to a pointer.
- `map_type` matches `HirType` exhaustively with no wildcard, so a new HIR variant is a compile error here.
- The module is run through the MLIR verifier before its textual form is returned.

### Tensor arithmetic

A function whose body is a run of `val` bindings closed by one `return` or a tail expression, over element-wise `+ - * / %` on tensors and the operations below, is emitted as a definition. An operand may be owned or borrowed: a `&Tensor` or `&mut Tensor` parameter, or `&a` in the expression, reads the same elements. Each operator becomes a `tensor.empty` destination plus one `linalg.generic`:

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

Float elements use the `arith` float operations (`remf` for `%`, which is C's `fmod`). Integer elements use theirs, with division and remainder splitting on signedness (`divsi` / `divui`, `remsi` / `remui`), plus the checks described under [Linking into a compile](#linking-into-a-compile). A half-precision element (`f16`, `bf16`) is widened to `f32` for each operation and rounded back once, so `(a * b) + c` rounds twice, as two separate operations do.

Anything the builder cannot express leaves the function an external declaration rather than failing. Examples are a scalar body, which stays on the LLVM backend by design, an extent that neither matches the result's nor is 1, an operand of higher rank than the result, and a body that hands back an argument unchanged.

### Broadcasting

Operands need not share the result's shape. Each one gets its own indexing map, computed from its shape against the result's, and that map alone broadcasts:

| Operand | Map, for a `[2, 3]` result | Meaning |
| --- | --- | --- |
| `Tensor<f32, [2, 3]>` | `(d0, d1) -> (d0, d1)` | walks every axis |
| `Tensor<f32, [1, 3]>` | `(d0, d1) -> (0, d1)` | the size-1 axis is stretched: read at index 0 |
| `Tensor<f32, [3]>` | `(d0, d1) -> (d1)` | lower rank, aligned at the trailing axis |
| `f32` | `(d0, d1) -> ()` | a scalar, read at every point |

Shapes align at the **trailing** axis, so a lower-rank operand supplies the innermost axes and repeats across the leading ones. The destination always keeps the identity map, so the operation stays element-wise rather than becoming a gather.

A dynamic `?` extent lowers too: `tensor.empty` takes one size operand per dynamic axis, read back with `tensor.dim` on an operand that walks that axis. A stretched operand cannot supply one, since it is size 1 there and says nothing about the result. A `?` operand extent is never stretched either, because nothing can prove at run time that it is 1.

A scalar operand, a literal included, reaches an outlined body as a parameter. For an outlined operation a declaration is a compiler bug, which `neurc` reports at the operation.

### The other operations

Every one of these lowers for the host and for a GPU alike, except where noted.

- **Reductions** (`.sum()`, `.mean()`, `.max()`, `.min()`): a seed and a fold, one result element per parallel point folding its run in order. Runs longer than the language's 4096 reduction lanes are folded lane by lane first, as [GPU kernels](#gpu-kernels) describes. A half-precision run is widened to `f32`, folded there, and rounded back once.
- **Sorts** (`.sort()`, `.argsort()`, `.topk()`): a stable bottom-up merge sort, one parallel `linalg.generic` per doubling of the block width, each point placed by a binary search. On the host, an integer run of 256 or more sorts by LSD radix, a byte a pass, instead. Any stable sort under one comparator gives the same order, so the algorithm is never observable.
- **Elementwise math**: the `math` dialect op of each function's name, which the CPU pipeline turns into the same LLVM intrinsics the LLVM backend calls on a scalar.
- **Slices and permutations**: a gather reading the source at each result position, and an input map.
- **`einsum`**: the matrix product's contracting shape, adding in its letters' order.
- **Compound assignment**, on the host only: one `linalg.generic` over `memref`s whose destination is the `&mut` target's own buffer, so the update writes in place and nothing is copied.
- **Traversals** (`.map`, `.zip`, `.reduce`), on the host only: one `linalg.generic` whose body calls the traversal's function through its function value, element by element in row-major order.

A GPU runs compound assignments and traversals as per-thread launchers of its own. See [`@kernel` functions](#kernel-functions).

### Matrix multiplication

`@` takes a path of its own. A matrix product contracts an axis rather than walking one, so its index space has a third dimension that no operand of the result has. It emits three operations: a `tensor.empty`, a `linalg.generic` that fills it with the element's zero, and a second one that accumulates into the filled destination:

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

`(d0, d1, d2)` is (row, column, contracted), and the reduction iterator comes last because that is the order the maps number the dimensions in. The fill is required: a reduction reads its destination at every point, so the destination must hold the zero, and `tensor.empty` is undefined memory.

Named `linalg.matmul` and `linalg.fill` are not reachable (melior's ODS module generates from `LinalgOps.td` only), so all three generics go through one builder that takes its operand split, maps, iterators and body region as arguments. Every extent must be static: `tensor.dim` can recover a dynamic result axis but not the contracted one, which appears in no operand of the destination, so a `?` anywhere leaves the function a declaration.

### The contraction schedule

Lowered as it is, that contraction is the naive loop nest: one output element at a time, the contracted axis innermost, the right operand read down a column. Before `lower_for_link` and `lower_for_gpu` bufferize a module, `schedule.rs` runs a transform-dialect script over every contraction the builders tagged (`@`, and an `einsum` with a contracted letter), through `mlirTransformApplyNamedSequence`:

- **Host**: the result axes tile into a register block of 4 rows by 64 bytes of columns (16 `f32` or 8 `f64`). Each contracted letter becomes a loop of its own outside the block, an axis the block does not divide is peeled into one smaller static block, every block vectorizes, and the accumulator is hoisted out of the contracted loops so it stays in registers.
- **GPU**: the block is up to 4 × 4 elements per thread, sized to divide the result axes, as an `scf.forall` that the parallel-loop mapping turns into threads. A product one block would cover keeps the plain kernel.

Each element still adds its products one at a time, in contracted order, starting from the fill's zero, with the multiply and the add rounded separately. A block holds one accumulator per element and never splits an element's sum, so a scheduled product has exactly the bits of the naive nest, on the host and on a GPU. That rules out tensor-core MMA and a vendor BLAS, which both split the contracted extent or fuse the multiply into the add.

A half-precision contraction widens its operands to `f32`, contracts there, and rounds each result element once, so it is scheduled as an `f32` one. A body with integer overflow checks (the `-O0` tier) and an `einsum` operand that repeats a letter are left as the naive nest.

### MLIR to LLVM IR

`translate_to_llvm_ir` runs a real MLIR pass pipeline rather than emitting LLVM by hand:

1. `linalg-fuse-elementwise-ops` folds each operation whose one use is the next into it, so `(a + b) * c` becomes one loop nest and the sum is never stored. The fused body runs the same operations in the same order, and no multiply and add are contracted without fast-math flags, so the results are bit-for-bit the unfused ones.
2. `one-shot-bufferize{bufferize-function-boundaries=true function-boundary-type-conversion=identity-layout-map}` rewrites tensor values into `memref` buffers. The function-boundary flag is required: without it a `func.func` keeps `tensor` in its signature, which `func-to-llvm` cannot convert. The identity layout makes a parameter a plain row-major `memref`, which is what a DLPack buffer is.
3. `buffer-results-to-out-params{hoist-static-allocs=true modify-public-functions=true}` turns a returned buffer into a trailing parameter the caller allocates. With a static shape, the body writes straight into it.
4. `buffer-deallocation-pipeline` gives each buffer still allocated inside an owner a release.
5. `func.func(convert-linalg-to-loops)` turns the structured op into `scf` loops over element loads and stores. It is nested under `func.func` because that is the operation it is anchored on, and it must run *after* bufferization: against tensor operands it silently leaves the op alone.
6. `convert-vector-to-scf`, `expand-strided-metadata` and `lower-affine` take apart a scheduled contraction's vector transfers and tile views. Then `convert-scf-to-cf`, `convert-vector-to-llvm` and `finalize-memref-to-llvm` lower what the loops are made of, and `func-to-llvm`, `arith-to-llvm`, `cf-to-llvm`, `index-to-llvm` and `ub-to-llvm` take the rest into the `llvm` dialect. `convert-math-to-llvm` emits the `llvm.exp`, `llvm.log`, `llvm.tanh`, `llvm.pow`, `llvm.sqrt` and `llvm.fabs` intrinsics the LLVM backend calls, so host math keeps its bits.
7. `reconcile-unrealized-casts` clears the `unrealized_conversion_cast` ops each conversion leaves at its boundary with the dialects the others own. The translation rejects any that survive, so this pass runs last by necessity.
8. `mlirTranslateModuleToLLVMIR` builds the LLVM module. melior does not wrap it, so the call goes through `mlir-sys` directly, pinned to the exact version melior depends on so both reach one crate instance.
9. The resulting `LLVMModuleRef` is wrapped by `inkwell::module::Module`, which owns it and disposes of it on drop, and is put through LLVM's verifier.

The pipeline is named in text and parsed with `melior::utility::parse_pass_pipeline`, because two of its entries have no usable typed constructor. melior's `one-shot-bufferize` takes no options, so it cannot set `bufferize-function-boundaries`, and `buffer-deallocation-pipeline` is a pipeline rather than a pass. Textually named passes must be in the process-global registry, so the MLIR context builder calls `register_all_passes` once.

A bufferized tensor parameter crosses as an exploded `memref` descriptor (allocated pointer, aligned pointer, offset, sizes, strides), and so does the result, as the last parameter. That is MLIR's tensor ABI, not the single DLPack handle the LLVM backend uses. The [LLVM backend's MLIR bodies](llvm-backend.md#mlir-bodies) section is where the two meet.

The `LLVMContext` that step 8 builds into is **inkwell's own**. `mlir-sys` and `llvm-sys` are independent bindings, and an install where they resolve to different `libLLVM` copies fails at this handoff instead of miscompiling downstream. Errors are values, one variant per stage: `PassPipelineFailed`, `TranslationFailed` and `LlvmVerificationFailed`.

### Linking into a compile

`lower_for_link` builds a second module holding the host body of every outlined operation and every `@gpu(fallback: true)` function. Each is defined under `__neuro_mlir_` plus its function's name. It returns that LLVM IR with the `(function, symbol)` pairs. A body qualifies when it lowers (above) and its signature is numbers, `bool`s, static tensors of them (owned or behind `&` / `&mut`), or function values over them. It returns a static tensor, a tuple of them, or nothing. A body with a tensor result marks its pointer parameters `noalias`: it only reads its operands and writes a buffer its caller has just allocated, which lets LLVM keep an accumulator in a register once the body is inlined. A compound assignment's symbol writes its target in place and has none. The module declares nothing, because a declaration would name a Neuro-ABI function at an MLIR signature.

The [LLVM backend](llvm-backend.md#mlir-bodies) emits each call to such a function as a call to its symbol, and links the IR in. The result buffer is the LLVM backend's allocation, so it is a DLPack tensor like any other. A buffer the body needs in between is allocated and freed inside it.

**Integer checks.** The LLVM backend panics on an integer overflow in a debug build and on a zero divisor in every build, naming the operator. A body here cannot panic, and on a GPU nothing can stop the program. A checked body therefore takes one more parameter, a `memref<1xi64>` status word that its caller fills with all ones. A failed check (an `scf.if` in the `linalg` body) lowers the word with an atomic unsigned min to a key: the operation's number on top, the element's row-major position in the middle, and the check's number, counted from 1, in the low 12 bits. The smallest key is the failure the host would have stopped at, whichever GPU thread got there first. `lower_for_link` returns each symbol's checks with it, and the LLVM backend panics for the one left in the word after the call. A failed divisor is replaced by 1 so the body stays defined. `neurc` passes `Overflow::Checked` only at `-O0`, so overflow checks exist only in a debug build. A body with more than 4095 checks stays on the LLVM backend.

### GPU kernels

`lower_for_gpu` takes the module `lower_for_link` builds and lowers each `linalg` op to a GPU kernel instead of a loop nest. `GpuTarget::Nvidia { chip }` goes through `nvvm` and embeds PTX, which the CUDA driver compiles for the GPU it runs on, so compiling needs no CUDA toolkit. `GpuTarget::Amd { chip }` goes through `rocdl` and embeds a code object for exactly that chip. Building the code object runs ROCm's `ld.lld`, so an AMD target needs ROCm installed and fails with `GpuSerializationFailed` without it. The chip must be one LLVM 23 has a processor model for, or it is `InvalidGpuChip`.

The parallel loops are tiled with a bounds guard rather than a clamped extent, so blocks cover the grid and each block runs a tile of threads. The tiles fit each function's rank. For ranks 1 to 3 the innermost axis goes on thread x, 32 wide, so a warp reads contiguous memory, with 256 threads a block (`256`, `8 × 32`, `1 × 8 × 32`). Rank 4 and up, and a tensor whose outer axis is too long for grid y or z (which hold 65,535 blocks), keep a 16 × 16 tile over the first two axes, with the outermost axis on grid x. Functions that need different tilings lower as separate modules.

A matrix product is two kernels, the zero fill and the contraction, whose threads each compute a block of the result ([the contraction schedule](#the-contraction-schedule)). A reduction (`.sum()`, `.mean()`, `.max()`, `.min()`) is a seed and a fold, plus a division for a mean. One thread per result element folds its run in order, which is the LLVM backend's order, so the device and host answers match exactly. A run longer than the language's 4096 reduction lanes first folds into a partials tensor, one thread per lane and per result element, each looping over its lane's run positions. The seed and fold then reduce the lanes, in the lane order every backend shares. That puts a whole-tensor `.sum()` on thousands of threads.

A GPU body also lowers elementwise math, slices, permutations and `einsum`, as the host does. A slice position is a literal, or a parameter the call site has already checked, since a GPU body cannot stop the program. A body over a half-precision or `bool` tensor stays off the GPU, because neither has a device kernel checked against it. A float `%` calls the device math library's exact `fmod`, as the math functions below do, rather than the hardware's divide-based `frem`.

A math function becomes a call into the GPU vendor's device math library (libdevice on NVIDIA, ocml on AMD). The kernels can link it only when the CUDA toolkit or ROCm is found while compiling. `lower_for_gpu` probes for it once, only for a program whose GPU code calls one. Without it, such a body stays on the host (an outlined operation) or is refused (`@gpu`, `@kernel`). `.abs()` is the exception: a device computes it by clearing the sign bit, which is the host's `fabs` exactly and needs no library.

Each symbol keeps `lower_for_link`'s signature, so the [LLVM backend](llvm-backend.md#mlir-bodies) wrapper serves either path. The two paths split the program by `HirFunction::target`. `lower_for_link` takes the outlined operations and the fallback functions' host copies. `lower_for_gpu` takes the `@gpu` functions, with or without a fallback, and the outlined operations of a program that moves a tensor to a device. Device symbols are named `__neuro_gpu_` plus the function's name, so a function's host and device bodies link side by side. A `@gpu` body that `lower_for_link` would not take, or one with a rank-0 tensor in it (a rank-0 operation has no parallel axis to launch over, so it would run on the host against device buffers), is `GpuBodiesNotLowered`. `@gpu` forbids running it anywhere but a GPU. A `FollowsOperands` body it cannot lower keeps its host body alone.

Every buffer a symbol is handed must be device memory. The LLVM backend's wrapper stages them. A buffer the body needs between two kernels, such as the product in `(a @ b) * c`, is allocated through `_mlir_memref_to_llvm_alloc` and freed through `_mlir_memref_to_llvm_free`, not `malloc` and `free`. The LLVM backend defines both as its device allocator. The symbol's body calls MLIR's GPU runtime ABI (`mgpuModuleLoad` or `mgpuModuleLoadJIT`, `mgpuLaunchKernel`, the `mgpuStream*` calls), which the IR declares and the LLVM backend's runtime defines.

### @kernel functions

A `@kernel` body is the per-thread code itself, so it does not go through `linalg`. The [`kernel`](../../../compiler/mlir-backend/src/kernel/mod.rs) module writes each one as MLIR text: a `func.func` whose tensor parameters are `memref`s, a grid sized from the first `&mut` tensor's extents and the `threads` block shape, and a `gpu.launch` whose region is the body. Locals are `memref.alloca` slots hoisted to the region's entry, and control flow is `cf` branches, so a loop needs no loop-carried values and `break` or `return` is a plain branch.

Every tensor index is bounds-checked. Every integer divisor is tested for zero, and in a debug build every integer `+ - *` is tested for overflow. NVIDIA stops the thread with `cf.assert` (a device assertion with a message). AMD, whose `rocdl` lowering has no `cf.assert`, traps instead. The block size is written as a constant, because `gpu.block_dim` lowers to a ROCm device-library call on AMD.

The launchers use a pipeline of their own: outlining, the vendor conversion and the shared descent to the `llvm` dialect, without the bufferization prefix. Their buffers are the caller's from the start, and the deallocation pass that follows bufferization refuses a body that branches in a loop. The `@gpu` module and the kernel module are translated separately and linked into one. A construct the body lowering does not cover (a function call, a slice, a tuple) is `KernelBodiesNotLowered`, with the construct's span and what it is.

The same per-thread lowering runs the outlined operations that `linalg` cannot express: a compound assignment (one thread per element, writing the target's own buffer), and `.map` / `.zip` (one thread per element, calling the closure inline). `.reduce` folds in a single thread, in the host's order. Integer arithmetic there reports a failed check through the same status word and stops the thread. In a `@kernel` it stops the kernel instead, as a bad index does. A body the lowering refuses keeps its host copy rather than becoming an error. See [`kernel/body/traversal.rs`](../../../compiler/mlir-backend/src/kernel/body/traversal.rs).

## Diagnostics

`MlirError` covers the failures below. The messages are the ones a user sees when the compile stops.

| Variant | Message | Cause |
| --- | --- | --- |
| `UnsupportedType` | `unsupported HIR type for MLIR lowering: {type}` | a type with no MLIR form in a position that needs one |
| `PassPipelineFailed` | `MLIR conversion to the llvm dialect failed` | a pass in the bufferization or descent pipeline rejected the module. This is a lowering bug. |
| `TranslationFailed` | `MLIR could not be translated to LLVM IR` | MLIR's translation to LLVM IR failed |
| `LlvmVerificationFailed` | `translated LLVM module failed verification: {detail}` | the translated module is not valid LLVM IR |
| `ModuleVerificationFailed` | `MLIR module failed verification` | the MLIR module is not valid |
| `InvalidGpuChip` | `unknown GPU chip `{chip}`: expected a processor LLVM 23 knows, such as `sm_80` or `gfx90a`` | a `--gpu-arch` value LLVM does not know |
| `GpuSerializationFailed` | `GPU kernels could not be serialized; an AMD target needs ROCm installed` | an AMD build without ROCm's `ld.lld` |
| `GpuBodiesNotLowered` | `no GPU kernel for `@gpu` {names}: the GPU path lowers straight-line tensor code ...` | a `@gpu` body outside the GPU subset, with each function's span |
| `KernelBodiesNotLowered` | `no GPU kernel for `@kernel` {names}` | a `@kernel` body with a construct the kernel lowering lacks |
| `AttributeSyntax` | `MLIR rejected the generated attribute `{attribute}`` | an internal attribute the builders wrote, which MLIR did not parse |

Two cases are not errors. A function the builder cannot express stays a declaration (see [Tensor arithmetic](#tensor-arithmetic)), and a tensor operation outlined out of host code that has no body is a compiler bug. `neurc` reports that one as `internal error: no MLIR body computes the tensor operation`.

## Testing

- Unit tests sit beside the code in `#[cfg(test)]` modules, in files such as [`gpu.rs`](../../../compiler/mlir-backend/src/gpu.rs), [`bridge.rs`](../../../compiler/mlir-backend/src/bridge.rs), [`schedule.rs`](../../../compiler/mlir-backend/src/schedule.rs), and the `tensor_*` modules, plus the `kernel/` tree.
- End-to-end behavior is covered by the `neurc` suites in [`compiler/neurc/tests/suite/`](../../../compiler/neurc/tests/suite/). The tensor files (`tensor_matmul.rs`, `tensor_reductions.rs`, `tensor_sort.rs`, and so on) exercise this crate through real programs. The `gpu_*.rs`, `kernel_*.rs` and `multi_gpu.rs` files cover the GPU paths.

Run the crate's tests with the three prefix variables set to one LLVM and MLIR 23 install, as [Installation](../../getting-started/installation.md#mlir) describes:

```bash
cargo test -p mlir-backend
```

## Source

- [`compiler/mlir-backend/src/lib.rs`](../../../compiler/mlir-backend/src/lib.rs)
- [`compiler/mlir-backend/CONTEXT.md`](../../../compiler/mlir-backend/CONTEXT.md): the authoritative contract, with the rationale behind each rule

## See Also

- [LLVM Backend](llvm-backend.md), which emits the scalar code and links these bodies in
- [HIR Lowering](hir-lowering.md), which outlines each tensor operation into its own function
- [Installation](../../getting-started/installation.md#mlir), for building MLIR 23
- [melior](https://github.com/raviqqe/melior), Rust MLIR bindings, and [MLIR](https://mlir.llvm.org/)
