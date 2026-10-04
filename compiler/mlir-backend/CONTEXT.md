# mlir-backend

## Purpose
Compute every tensor operation, on the CPU and as NVIDIA or AMD GPU kernels: the LLVM backend has no tensor loops of its own. It consumes `neuro_hir::HirProgram`, in which lowering has outlined each tensor operation into a function of its own, and builds each one's body from the `linalg`, `tensor` and `memref` dialects. The module carries on through bufferization and the `llvm` dialect into a verified inkwell LLVM module, handed to the driver as linkable LLVM IR, which `neurc` links into every compile. `lower_program` still emits the debugging view: a `func.func` declaration per function, or a definition where the body is one this crate builds.

## Toolchain
Every build compiles the crate, so every build needs MLIR 23 in the LLVM 23 prefix (see
Notes, toolchain pinning). CI provisions it on all three OSes: apt.llvm.org on Linux,
Homebrew's `llvm` on macOS, and on Windows a static source build the `setup-llvm` action
caches.

## Entry Points
- `lower_program(&HirProgram) -> Result<String, MlirError>`: walks the typed HIR and returns
  the textual form of a verified module of `func.func` declarations.
- `translate_to_llvm_ir(&HirProgram) -> Result<String, MlirError>`: the same module carried on
  through a bufferization and conversion pipeline into the `llvm` dialect, translated into an
  inkwell LLVM module, LLVM-verified, and returned as textual LLVM IR.
- `lower_for_link(&HirProgram, Overflow) -> Result<LinkableBodies, MlirError>`: the driver's
  entry. The host body of every outlined function (`HirTarget::FollowsOperands` or
  `HostOperation`) and of every `@gpu(fallback: true)` function's host copy, each defined as
  `__neuro_mlir_<function>`, carried through the same pipeline and returned as LLVM IR with its
  `(function, symbol)` pairs and each checked symbol's `Guard`s (see Notes, integer checks).
  `Overflow::Checked` is the debug tier, where integer overflow is a failed check; `Wrapping` the
  release tier. Empty IR and no pairs when nothing qualifies. A symbol with a tensor result has
  `noalias` on its pointer parameters: it only reads its operands and writes a buffer its caller
  has just allocated, and saying so lets LLVM keep an accumulator in a register once the body is
  inlined. A compound assignment's symbol writes its target in place and has none.
- `lower_for_gpu(&HirProgram, &GpuTarget, Overflow) -> Result<LinkableBodies, MlirError>`: every `@gpu`
  function, `fallback: true` ones included, with the pairs and symbol signatures `lower_for_link` would give it, each symbol
  named `__neuro_gpu_<function>` so a host body and a device body link side by side, but each symbol
  launches its `linalg` ops as GPU kernels for `GpuTarget::Nvidia { chip }` (`nvvm`, PTX) or
  `GpuTarget::Amd { chip }` (`rocdl`, a code object). A `@gpu` body that would not reach
  `lower_for_link`, or that has a rank-0 tensor, is `GpuBodiesNotLowered` with every such
  function's name and span: running it on the host is what `@gpu` forbids. Every buffer a symbol
  is handed must be device memory, and it allocates a buffer between two kernels through
  `_mlir_memref_to_llvm_alloc` / `_mlir_memref_to_llvm_free`, which the caller defines. It also
  admits every `FollowsOperands` function (a tensor operation lowering outlined out of host code)
  the same way, but leniently: one it cannot lower keeps its host body alone and is no error. A
  body over a half-precision or `bool` tensor is kept off the GPU (`exact_on_gpu`): neither has
  a device kernel checked against it. A float `%` counts as a math call (`math_functions`): the
  vendor conversion turns `arith.remf` into the device library's exact `fmod`, so it needs that
  library exactly as `exp` does. A `@gpu` body with one is refused as before. One
  whose body is a compound assignment or a `.map` / `.zip` / `.reduce` becomes a per-thread
  launcher instead (`kernel/body/traversal.rs`), as leniently. It
  lowers every `@kernel` function (`HirTarget::Kernel`) to a symbol of the same shape that
  returns nothing and launches the body once per thread; a body construct the kernel lowering
  lacks is `KernelBodiesNotLowered`, a `KernelRefusal` (function, construct span, what it is) per
  function. A body calling a math function needs the vendor's device math library; without it
  (probed once, see Notes) a `FollowsOperands` one keeps its host body, a `@gpu` one is
  `GpuBodiesNotLowered` and a `@kernel` one a `KernelRefusal` at the call. `neurc` calls it for a
  program with a `@gpu`, `@kernel` or `FollowsOperands` function.
The HIR-independent wiring check that used to sit beside them, `emit_smoke_module`, is gone
from the public surface: `build_smoke_module` is `pub(crate)` and compiled only under `test`,
because the Phase 1.8 condition it was written for ("until real HIR lowering exists") is met
and nothing outside the crate ever called it. It still builds
`func.func @neuro_smoke(index, index) -> index` with a single `arith.addi` body, and `bridge`'s
tests still carry that module across to LLVM IR, because it is the one module with a real body
rather than only declarations.

## Shared Kernel
- `neuro-hir`: the typed HIR contract `lower_program` consumes.
- `ast-types`: `BinaryOp`, which the HIR's `Binary` expression carries rather than redeclaring.
- `shared-types`: `Span`, which `GpuBodiesNotLowered` carries per refused function.

The crate adds no business logic of its own beyond the lowering; it otherwise uses only
third-party `melior` + `mlir-sys` + `inkwell` + `thiserror`.

`syntax-parsing` and `hir-lowering` are `[dev-dependencies]` only: the `@kernel` tests build
their HIR from source. Never a production dependency.

## Notes
**The MLIR → LLVM crossing.** `translate_to_llvm_ir` runs `llvm_lowering_pipeline()`, named in
text (its two halves, the `BUFFERIZE` constant and `llvm_descent`, are shared with the GPU pipeline, which passes the descent its own `finalize-memref-to-llvm` spelling) and parsed by `melior::utility::parse_pass_pipeline`: melior's typed `one-shot-bufferize`
constructor takes no options, and `buffer-deallocation-pipeline` is a pipeline with no
constructor at all. It opens with `linalg-fuse-elementwise-ops`, which folds an operation whose one
use is the next into it, so `(a + b) * c` is one loop nest (one kernel on a GPU) with no buffer for
the sum; the fused body runs the same operations in the same order, and neither backend contracts a
multiply and an add without fast-math flags, so the bits do not change. The next four entries are
what carry a `linalg` body: `one-shot-bufferize` (with
`bufferize-function-boundaries=true`, or a `func.func` keeps `tensor` in its signature and never
converts, and `function-boundary-type-conversion=identity-layout-map`, so a parameter is a plain
row-major `memref` and a copy into one lowers to `llvm.memcpy` rather than to a runtime-library
call nothing links) rewrites tensor values into `memref` buffers, `buffer-results-to-out-params`
(`modify-public-functions`, since every definition is public, and `hoist-static-allocs`, so a
static result is written straight into the caller's buffer) turns each returned buffer into a
trailing parameter, `buffer-deallocation-pipeline` gives every buffer still allocated inside an
owner, and only then does `convert-linalg-to-loops` (nested under `func.func`, which is
what it is anchored on) produce `scf` loops; run before bufferization it silently leaves the op
alone. The rest is the descent those loops land in: a scheduled contraction's vector transfers
unrolled (`convert-vector-to-scf`) and its tile subviews turned into index arithmetic
(`expand-strided-metadata`, `lower-affine`), then `convert-scf-to-cf`, `convert-vector-to-llvm`,
`finalize-memref-to-llvm`, then `func` / `math` / `arith` / `cf` / `index` / `ub` to LLVM (`convert-math-to-llvm` emits the
`llvm.exp` / `llvm.log` / `llvm.tanh` / `llvm.pow` / `llvm.sqrt` / `llvm.fabs` intrinsics, the ones
the LLVM backend called, so host math keeps its bits) and `reconcile-unrealized-casts` last by necessity,
since each conversion leaves `unrealized_conversion_cast` ops at its boundary with the dialects the
others own and the translation rejects any that survive. Pass names in a textual pipeline must be in
the process-global registry, so `new_context` calls `register_all_passes` behind a `Once`.

It then calls `mlirTranslateModuleToLLVMIR`
**directly through `mlir-sys`**: melior does not wrap it, and `mlir-sys` is pinned to
the exact version melior itself depends on so both reach one crate instance and their
`MlirOperation` / `LLVMContextRef` types unify.

The `LLVMContext` the translation builds into is **inkwell's**, and the returned `LLVMModuleRef`
is wrapped by `inkwell::module::Module` (sole owner, disposes on drop) and put through LLVM's
verifier. That is the whole point of the entry point: `mlir-sys` and `llvm-sys` are independent
bindings, and a build where they resolve to different `libLLVM` copies fails at this handoff
rather than miscompiling later. Each binding declares its own opaque `LLVMContextRef` /
`LLVMModuleRef` alias over the same C type, so the pointers are cast across.

`register_all_llvm_translations` runs in `new_context()` for every path, not only the translating
one: the translation interfaces have to be on the context that *built* the module.

**Toolchain pinning.** `melior 0.28.x` is the MLIR 23 line (via `mlir-sys 230`). `mlir-sys` carries no `llvm-sys` dependency and no Cargo
`links` key clashing with inkwell's. It finds MLIR by running
`$MLIR_SYS_230_PREFIX/bin/llvm-config`, so MLIR has to be installed into the same prefix as the
LLVM that `LLVM_SYS_231_PREFIX` names; `TABLEGEN_230_PREFIX` names that prefix too. One prefix
means both bindings load one `libLLVM` 23, which the crossing above depends on. On Ubuntu,
apt.llvm.org's `libmlir-23-dev` installs MLIR beside LLVM under `/usr/lib/llvm-23`. Homebrew's
`llvm` builds MLIR in. Arch has no package that fits (`aur/mlir` builds no `libMLIR-C`) and no
Windows release carries MLIR, so on both it is a static source build of LLVM with MLIR in one
prefix. `mlir-sys` uses Rust 2024 let-chains in its build script, which the workspace MSRV
covers.

**The GPU pipeline.** `lower_for_gpu` builds the `lower_for_link` module and swaps the middle of
the CPU pipeline: `scf-forall-to-parallel` (a scheduled contraction's per-thread blocks),
`convert-linalg-to-parallel-loops`, `convert-vector-to-scf`, `scf-parallel-loop-tiling` (guarded rather
than clamped, so the outer loop maps to blocks and the inner to threads), `gpu-map-parallel-loops`,
`convert-parallel-loops-to-gpu`, `gpu-kernel-outlining`,
`gpu-async-region` (a body's launches chain on one stream with a single wait at its end, instead
of a stream created, waited on and destroyed per launch), then
`nvvm-attach-target` / `rocdl-attach-target` with the chip and `convert-gpu-to-nvvm` /
`convert-gpu-to-rocdl` inside each `gpu.module`, after `expand-strided-metadata`, `lower-affine`
and `convert-vector-to-llvm` there, since the vendor conversion handles neither a block's subviews
nor its vectors. `lower-affine` is added for the index arithmetic
the GPU mapping writes; `gpu-to-llvm` turns each launch into calls to MLIR's GPU runtime ABI
(`mgpuModuleLoad[JIT]`, `mgpuLaunchKernel`, `mgpuStream*`), which the IR declares and nothing in
this crate defines.

**Tiling, per function.** Tile sizes are read by loop axis from the outermost, so one list fits one
rank; `tiling` picks a function's from its widest tensor and functions that differ lower as
separate modules. Ranks 1 to 3 take `mapping-policy=innermost-first` (absent from LLVM 20), which puts
the innermost axis on thread x, 32 wide, so a warp reads 128 contiguous bytes: `256`, `8,32` and
`1,8,32`, 256 threads a block. The default policy put the outermost axis on x, so a warp strode
across rows, and a wide tensor asked grid y for more than its 65,535 blocks. An outer axis too long
for grid y or z in those tiles, and any rank past 3 (whose innermost axis no policy maps), keep
the old `16,16` outermost-first tiling, where the long axis sits on grid x. A loop of lower rank
than its function's tiling (a reduction's result) still launches, with fewer threads a block. `gpu-module-to-binary` runs as a second pass manager so a missing toolkit is
`GpuSerializationFailed` and a lowering bug stays `PassPipelineFailed`. NVIDIA embeds PTX (`isa`),
which the CUDA driver JITs for its GPU, so a compile needs no CUDA toolkit. AMD embeds a code
object (`bin`): HIP cannot load assembly, and linking one runs `$ROCM_PATH/llvm/bin/ld.lld`. The
chip must be one LLVM 23 has a processor model for (`NVIDIA_CHIPS` / `AMD_CHIPS`), or it is
`InvalidGpuChip`: LLVM only warns about an unknown one and then crashes selecting AMD
instructions, and the list also keeps anything but a plain name out of the pipeline text the
chip is spliced into. The host symbol keeps `lower_for_link`'s exploded-descriptor signature, so one
LLVM-backend wrapper serves either path; the pointers it passes must be device memory, which the
wrapper stages.

**`@kernel` launchers.** `kernel/mod.rs` writes each `@kernel` function as MLIR text, parsed
with `Module::parse`: a `func.func` taking `memref`s for its tensors, the grid (per axis,
`ceil(extent / threads)` blocks over the first `&mut` tensor's extents, 1 past its rank; no launch
at all for a zero extent), and a `gpu.launch` whose region `kernel/body.rs` emits. Text rather
than builders because `gpu.launch` has segmented operands and a twelve-argument region. This is
device code the LLVM backend cannot emit, so it is not a second host scalar codegen. Locals and
the values of `if` / `&&` / `||` / blocks are `memref.alloca` slots hoisted to the region's
entry; control flow is `cf` branches, so loops carry nothing in SSA and `break` / `continue` /
`return` are branches (`return` to the block holding `gpu.terminator`). Every tensor index is
widened to `i64` (sign-extended when signed, so a negative one fails the same `ult` test) and
bounds-checked. Integer arithmetic has the host's checks (`checked_int`): a zero divisor on every
tier, and an overflow (through LLVM's `with.overflow` intrinsics) and `MIN / -1` on the debug
tier, where the release tier wraps; a failed one stops the kernel (`Failure::Stop`). The guard is `cf.assert` on NVIDIA and a trap block on AMD, whose ROCDL lowering
has no `cf.assert`; the trap block branches on, since `gpu.launch` wants every exiting block to
end in `gpu.terminator`. `thread_id` is `block_id * threads[axis] + gpu.thread_id` with the block
size as a constant, because `gpu.block_dim` lowers to a ROCm device-library call. Float to integer
casts saturate through `llvm.call_intrinsic "llvm.fptosi.sat..."`, as on the host. A `KernelPartition` runs its body in each thread whose global position is
inside the grid tensor; the thread's number is that position read row-major, and its run of
`out` starts at `number * chunk`, `chunk` being `out`'s element count over the grid tensor's (a
count the grid cannot share is a refusal, which reaches only a generic instance, the checker
having caught the rest). `slice` is a `Binding::Slice`: `slice[i]` is checked against `chunk`
and then turned from `base + i` back into one index per axis with `divui` / `remui` by constant
strides (no `memref.collapse_shape`), and `slice.len()` is the constant `chunk`. The body's
`return` branches to the block after the partition. The launchers run a pipeline of their own (`kernel_lowering_pipeline`: outline,
async region, vendor attach and conversion, the shared descent) with no bufferization prefix,
because `buffer-deallocation-pipeline` refuses unstructured loops and `gpu.launch_func`; the
`@gpu` module and the kernel module are translated separately and linked in one LLVM context
(`translate_llvm_dialects`).

**GPU memory.** The descent runs `finalize-memref-to-llvm{use-generic-functions=true}`, so a buffer
bufferization allocates between two kernels (the product in `(a @ b) * c`, since a contraction
does not fuse into its consumer) calls `_mlir_memref_to_llvm_alloc` / `_mlir_memref_to_llvm_free`
instead of `malloc` / `free`, and the LLVM backend defines those as its device allocator. Only the kernels read or write these buffers.
The exception would be an operation with no parallel axis, which has no loop to map
and so runs on the host against device buffers; only a rank-0 operation has none, and a rank-0
operation needs rank-0 operands, which come from the parameters. `launches_every_op` therefore
keeps a body with a rank-0 tensor parameter or result off the GPU module, through the admission
rule `build_linkable_module` takes (the CPU path admits every `Host` function, the GPU path every
`Gpu` one that passes this test), and `refused_bodies` turns what is left out into the error.

**Reductions.** `tensor_reduce::build_reduce` lowers `.sum()` / `.mean()` / `.max()` / `.min()`,
for the host and a GPU alike. A half-precision source is widened to `f32` first
(`cast_elements`), folded there and the result rounded back once, so a long sum does not stall
where a 16-bit total stops growing. A rank-0 result (a rank-1 axis reduction) has no parallel
axis and so stays off a GPU. The index space is the result's axes
(`parallel`, so one GPU thread per result element) followed by the reduced ones (`reduction`, a
sequential loop inside the thread), which folds each run in the source's order, as the LLVM
backend does, so the two give the same bits. A run longer than `neuro_hir::REDUCE_LANES` folds in
the language's lane order instead: `build_lanes` writes a partials tensor of the result's axes
and a lane axis with one all-parallel `linalg.generic`, whose body runs an `scf.for` over the
lane's run positions (`lane + k * REDUCE_LANES`, read with `tensor.extract`, the lane's own first
element seeding it and a position past the run's end keeping the accumulator), and the seed and
fold below then reduce the lane axis. The lane axis is innermost, so a whole-tensor reduction's
warp reads contiguous memory. The generic has no input, so `linalg-fuse-elementwise-ops` cannot
fold it into the lane fold and put it back on one thread. The destination is seeded first: `-0.0` for a sum
(the additive identity, signed zeros included), the run's first element for `.max()` / `.min()`,
whose fold keeps the element when it is a number that sorts before the accumulator or the
accumulator is NaN (the LLVM backend's sorting comparator). A mean divides by the run length in
a third generic. A whole-tensor reduction arrives boxed in a one-element `TensorLiteral` (a GPU
body returns buffers only) and becomes a reduction into `tensor<1xT>` with one parallel axis of
extent 1. An integer `.sum()` seeds 0, folds left to right with the overflow check
and never in lanes, since the host folds an integer run in order and where an overflow is caught
depends on that order.

**Sorts.** `tensor_sort::build_sort` lowers `.sort()` / `.argsort()` / `.topk()` by building
the stable order of every run (a tensor of run positions shaped like the source), then one
all-parallel gather per output that reads through it: the element, or the position as
`index_cast` to `i32`. `.topk` gathers only its `k` positions. Any stable sort under one
comparator gives the same order, so the algorithm is never observable.

- *Merge sort, host and GPU.* Bottom-up, one all-parallel `linalg.generic` per doubling of the
  block width (`ceil(log2(extent))` of them, none for a run of one). Each point finds the run
  position it holds by a merge-path binary search over the pair of blocks it merges, a fixed-count
  `scf.for` inside the body, reading the previous order and the source with `tensor.extract`.
  A right element goes first only if it strictly precedes (`precedes`, the LLVM backend's
  comparator with NaN last, which the `.max()` / `.min()` fold also uses), which keeps it stable.
  O(n log² n) comparisons a run.
- *LSD radix, host integers only.* A run at least `RADIX_BUCKETS` (256) long on the host
  (`Lowering::side`) sorts one byte a pass in `scf.for` loops over tensor values (count, prefix,
  place), two order tensors written in turn. The key flips the sign bit when signed and every bit
  when descending. A device body cannot take it: loops outside `linalg` would run on the host
  against device memory, and a radix pass in `linalg` needs a scan and a scatter.

A `.topk` body returns two tensors, so a defined function's tuple of tensors
is one MLIR result per tensor, each its own out-param after bufferization, and `linkable_result`
admits it, as it admits the `i32` indices tensor a sort writes.

**Math, layouts and contractions.** `build_expression` also lowers elementwise math
(`tensor_math.rs`: one `linalg.generic` whose body is the `math` dialect op of the function's
name, `.pow`'s exponent a scalar every point reads, `sign` two compares; a half-precision operand
is widened to `f32` and the result rounded back, as the LLVM backend computed a scalar one), slices and permuting
shape casts (`tensor_layout.rs`: a permute is an input map; a slice is a generic with no inputs
whose body reads the source with `tensor.extract` at `start + d * step`, from `end - 1` down when
reversed, and a position axis at one index) and `einsum` (`tensor_einsum.rs`: output letters
`parallel`, contracted letters `reduction` in letter order, a `+0.0` fill, operands multiplied left
to right and added to the accumulator, the LLVM backend's order exactly; a full contraction
arrives boxed in `[1]` and gets one leading parallel dimension of extent 1). A slice position is a
literal inside its axis, or, only for an outlined target, an integer parameter the call site has
checked, since a GPU body cannot stop the program.

**Per-thread launchers for outlined bodies.** A `FollowsOperands` function whose body is a compound
assignment (through `*__operand1`) or a traversal is written as MLIR text like a `@kernel`
(`kernel/body/traversal.rs`, a child of `body.rs` so it shares `BodyEmitter`; the traversal's
function value, the host body's way to call it, is a launcher parameter left unread): a compound
assignment and a `.map` / `.zip` run one thread per element (256 a block, the position row-major
delinearized), each reading its element(s), and either updating the `&mut` target in place (the
value read with the host's trailing-axis broadcast) or calling the closure and storing to the
result out-param; a `.reduce` runs one thread folding the closure over the buffer in row-major
order into a `[1]` out-param. The closure is the lifted `HirClosure` the body's `Closure`
expression names, its parameters bound to the loaded elements and its captures to the launcher
parameters of the same name. The emitter runs with `Failure::Report` there: a failed integer
check lowers the launcher's `%status` word (a parameter between the others and the result) to its
key and the thread leaves the launch, and `return` with a value stores into the call's result
slot. A body it refuses leaves the function to the host. `BodyEmitter`
also lowers scalar math functions (`math.*`, `sign` as selects) for `@kernel` bodies.

**The device math probe.** A `math` op inside a `gpu.module` becomes a call into libdevice (NVIDIA)
or ocml (AMD), which `gpu-module-to-binary` links only when it finds the CUDA toolkit
(`CUDA_ROOT` / `CUDA_HOME` / `CUDA_PATH`, or MLIR's build-time default) or ROCm; with no toolkit the
PTX keeps `.extern .func __nv_*`, which the driver refuses at load along with every other kernel.
`lower_with_format` therefore checks each built definition's text for `math.` (`math_functions`)
and, only then, serializes `MATH_PROBE` (one `math.exp`) with diagnostics swallowed: the library is
there when that succeeds with no `.extern` and no `__ocml_` left. Without it the linkable module is
rebuilt without the functions that call math (so `build_linkable_module`'s `admit` is a
`&dyn Fn`), and `kernel_launchers` re-emits a `@kernel` body with math refused. The answer is
cached for the compile in a `OnceCell`.

**Tensor arithmetic and the bodies above.** `tensor_arithmetic::build_body` turns a
function whose statements are `val` bindings and a final `return` or tail expression over the
operations above and element-wise `+ - * / %` on tensors into a `func.func` definition. An
operand may be borrowed: a `&Tensor` or `&mut Tensor` parameter is a tensor block argument
(`read_type`, which also gives the defined function's signature), and `&a` lowers to `a`, since
reading is all an operand does. A body that hands back one of its arguments unchanged is refused,
since it performs no arithmetic and linking it would copy a buffer the LLVM backend returns as is.
`build_body` takes a `Side`: for the host it first tries the two bodies a GPU cannot run, below. The
definition is one `tensor.empty` destination plus one
`linalg.generic` per operator, with one indexing map per operand, all-`parallel` iterators, and
an `arith` body terminated by `linalg.yield`. `@` is the exception and is described below. Every
element goes through `Lowering::arith` (`guards.rs`): a float one is the `arith` float operation
(`remf` for `%`), an integer one adds the LLVM backend's checks (`%` reports a remainder by zero
where `/` reports a division by zero), and a half-precision one (`Element::Half`) widens both
operands to `f32`, computes there and rounds the answer back, once per operation, as the LLVM
backend computed one.

**Compound assignment, on the host.** `tensor_compound.rs` turns a body that is one
`TensorCompoundAssign` through its `&mut` parameter into a `linalg.generic` over `memref`s: the
value (a buffer, or a scalar every element reads) in, the target's own buffer out, each element
computed with the host's checks, the target's element on the left. A tensor is a value, so writing
one in place would mean trusting bufferization not to copy it; a `memref` is the buffer the
`&mut` names. The function returns nothing.

**Traversals, on the host.** `tensor_apply.rs` turns a body that is one `.map` / `.zip`, or a
`.reduce` boxed in `[1]`, into a `linalg.generic` whose body calls the traversal's function
through its value: the outlined function's one parameter of function type, a
`!llvm.struct<(ptr, ptr)>` split once outside the loop, the environment passed ahead of the
elements, as the LLVM backend calls a function value. The function is the LLVM backend's code, so
the generic's row-major loops are the order a function with side effects sees the elements in.
`.reduce` seeds a `[1]` destination with `init` and folds every source axis as a reduction.

**Integer checks.** The LLVM backend panics on an integer overflow on the debug tier and on a zero
divisor on every tier, naming the operator. A body here can neither panic nor render a location,
and a GPU cannot stop the program at all, so a checked body takes a `memref<1xi64>` status word
after its parameters (added to its entry block on first use, so the defined signature comes from
the block's arguments). Its caller fills the word with all ones; a failed check (an `scf.if` in
the `linalg` body) lowers it with `memref.atomic_rmw minu` to a key: the operation's number on top,
the element's row-major position in the middle (a division's alone, the one operation whose two
checks report different diagnostics) and the check's number, counted from 1, in the low 12 bits.
The min therefore keeps the failure the host meets first (operations run one after another,
elements in row-major order), whichever thread wrote it. The checks travel as
`LinkableBodies::guards` and the caller panics for the one left. A failed divisor is replaced by 1
so the body stays defined; a body with more than 4095 checks stays the LLVM backend's.
`linalg-fuse-elementwise-ops` does not erase a fused producer whose body has a check, since the
`scf.if` store is a side effect, so its checks run once more in a loop of their own.

**Broadcasting is per-operand indexing maps.** Operand shapes align at their *trailing* axis,
so an operand of lower rank supplies the innermost axes and its map simply omits the leading
result dimensions. An extent of 1 against a larger result extent is stretched: that axis maps
to the constant `0`, so the operand is read at index 0 at every point the result axis covers. A
scalar operand of the element type has no index space at all and maps to `()`, which is how
`linalg.generic` hands one value to every point. A scalar operand is a parameter or, in a `@gpu`
body (lowered whole, without the outliner that makes host constants parameters), an `f32`, `f64`
or integer literal, which becomes an `arith.constant`. The destination always keeps the identity map;
that is what makes the operation element-wise rather than a gather. Any other mismatch (an
extent neither equal nor 1, an operand outranking the result, or a different element type) is
a shape error the frontend owns, so it answers `Ok(None)` rather than being lowered wrongly.

**A `?` extent sizes the destination from an operand.** `tensor.empty` needs one `index`
operand per dynamic axis, and those come from `tensor.dim` on an operand that walks that axis
itself. A *stretched* operand cannot supply one: it is size 1 there and says nothing about the
result. For the same reason a `?` operand extent is never stretched (nothing here can prove it
is 1 at run time, and guessing wrong would silently read the wrong element) so a result axis
no operand walks leaves the function a declaration.

It answers `Ok(None)`, meaning "leave this function a declaration", for anything else. Scalar
code stays the LLVM backend's: an outlined body reads its scalars, literals included, as
parameters, so nothing here lowers a literal. For an outlined function `None` is a compiler bug,
which `neurc` reports at the operation.

**A matrix product is a contracting `linalg.generic`.** `build_matmul` reads `@` instead of the
element-wise path and emits the canonical three-operation shape: a `tensor.empty`, a
`linalg.generic` that fills it with the element's zero from an `arith.constant` scalar input, and
a second one whose index space is `(row, column, contracted)` with maps `(d0, d2)` / `(d2, d1)` /
`(d0, d1)`, iterators `parallel, parallel, reduction`, and a multiply-accumulate body. The fill is
not optional: a reduction READS its destination at every point, and `tensor.empty` is undefined
memory. Named `linalg.matmul` and `linalg.fill` are still not reachable (melior's ODS module
generates from `LinalgOps.td` only) so all three go through the one `generic_op` builder, which
takes its operand split, maps, iterators and body region as arguments. Every extent must be
static here: `tensor.dim` can recover a dynamic result axis but not the contracted one, which
appears in no operand of the destination, so a `?` anywhere answers `Ok(None)`.

**Contractions are scheduled (`schedule.rs`).** `build_matmul` and `build_einsum` tag their
contracting generic with tile sizes (`neuro.tiles`, `neuro.peel`, `neuro.vectorize`) when its body
carries no integer check and no `einsum` operand repeats a letter. A half-precision contraction is
built over its operands widened to `f32` (`cast_elements`), accumulates in `f32` and narrows each
result element once at the end, so it is tagged as the `f32` contraction it is. `lower_for_link` and
`lower_for_gpu` call `schedule::apply` on the module they lower, before bufferization: it reads the
distinct tags back with a walk, writes one transform-dialect script and runs it through
`mlirTransformApplyNamedSequence` (melior wraps no interpreter). On the host the result axes tile
into a register block of 4 rows by 64 bytes of columns (`tile_using_for`), each contracted letter
gets a loop of step 1 outside the block, an axis the block does not divide is peeled, every block
vectorizes, and `hoist_loop_invariant_subsets` lifts the accumulator into the contracted loops'
iteration arguments, so it stays in registers. On a GPU the block is up to 4 x 4 per thread, sized
to divide its axes (`tile_using_forall`, never peeled), and a contraction one block would cover
whole is left untagged: a one-iteration `scf.forall` folds into its body and would run in the host
launcher. Each element still adds its products one at a time in contracted order from the fill's
zero, a separate multiply and add each rounded, so every result keeps the naive nest's bits on both
sides. No tensor-core MMA and no vendor BLAS: both split the contracted extent or fuse the multiply
into the add. The producer of an operand is never fused in (`structured.fuse` would recompute a
chained product per block), and the fill stays a loop of its own. `build_linkable_module` returns
the unscheduled module, which is what the construction tests read.

**The bufferized function has MLIR's tensor ABI, not Neuro's.** A tensor parameter crosses as an
exploded row-major `memref` descriptor (allocated pointer, aligned pointer, offset, then one size
and one stride per axis), a scalar as itself, and the result as one more descriptor after them;
the function returns nothing. The LLVM backend's tensor is one pointer to a DLPack handle, so the
two meet through a wrapper the LLVM backend emits: it defines the Neuro-ABI function as a call to
the linked symbol, passing each buffer out of its handle and a result buffer it allocated itself.
That keeps every allocation a Neuro tensor owns on the LLVM backend's side.

**What `lower_for_link` links.** `build_linkable_module` takes a free function only when
`linkable_signature` holds (every parameter a number, a `bool`, a static tensor of one, owned or
behind `&` / `&mut`, or a function value over them; a static tensor result, a tuple of them, or
nothing, see `linkable_result`) and `build_body` lowers it. An integer body carries its checks,
so it is as interchangeable as a float one. Static only, because the frontend gives a `?` axis no
arithmetic. The module declares nothing: a declaration would name a Neuro-ABI function at an MLIR
signature, and the linked IR is read for its definitions alone. The `__neuro_mlir_` prefix (and
`__neuro_gpu_` for a device body) keeps each symbol off the Neuro-ABI name the LLVM backend
defines.

**What `lower_program` emits.** It registers all dialects, then maps each top-level `HirItem`:
free functions, `impl` methods, and lifted closures become `func.func` *declarations* (empty
region, private visibility: external symbols, not definitions); structs, enums, and constants
carry no callable surface and are skipped. A lifted closure (`HirItem::Closure`, symbol
`__closure_N`) declares its captured-environment pointer as an implicit first parameter ahead
of the user-facing ones, matching the LLVM backend's calling convention. The module is run
through the MLIR verifier before its textual form is returned.

**Type mapping.** HIR scalars map to MLIR scalars (`i8`–`i64`, `i1` for `bool`, `i32` for
`char`, `f16`/`bf16`/`f32`/`f64`). Every aggregate / reference / string type, meaning tuples,
enums, and the standard collections (`Vec` / `HashMap` / `BTreeMap`), maps to an opaque `!llvm.ptr`.
A function value maps to `!llvm.struct<(ptr, ptr)>`, the LLVM backend's pair of the function and
its environment. A newtype is transparent: `HirType::Newtype`
maps to its inner type's mapping. `void` is the empty result list in return position and
`MlirError::UnsupportedType` anywhere else, as are the unsized types (`dyn Trait`, `[T]`),
which reach a value position only behind the reference that already maps to a pointer.
`HirType::Tensor` maps to a ranked MLIR tensor (`tensor<2x3xf32>`), the one aggregate that is
not an opaque pointer, because it is the one the dialects below operate on. A `?` axis becomes
MLIR's dynamic sentinel, read from `mlirShapedTypeGetDynamicSize` rather than written out. The
element must map to an MLIR integer or float; an aggregate element is `UnsupportedType`, since
`tensor<...>` does not accept `!llvm.ptr`. The LLVM backend's flat row-major buffer behind a
DLPack handle is what every descriptor this crate's symbols take points into.

`map_type` matches `HirType` exhaustively with **no wildcard**, so a new HIR variant is a
compile error here rather than a silent mis-map, on every build.
