# LLVM Backend

The LLVM backend turns the typed HIR into native object code, or into textual LLVM IR with `neurc compile --emit llvm-ir`. It owns all scalar code and every tensor operation with no loop, and links in the tensor loops that the [MLIR backend](mlir-backend.md) computes. Read it when a change touches code generation, the runtime ABI, or the panic and output paths.

**Crate**: `compiler/llvm-backend`
**Library**: inkwell 0.10.0 (LLVM 23 bindings)
**Build requirement**: `LLVM_SYS_231_PREFIX` pointing at an LLVM 23 install. Every build of the compiler also needs MLIR 23 in the same prefix ([Installation](../../getting-started/installation.md#mlir)).
**Entry points**: `compile` and `compile_to_ir`, both in [`lib.rs`](../../../compiler/llvm-backend/src/lib.rs)

## Overview

The backend reads the HIR produced by [HIR lowering](hir-lowering.md), not the AST. Lowering has already attached a resolved type to every expression, so codegen reads types inline and no backend pass collects them again. The language features it generates are described in the [language reference](../../language-reference/).

`compile` returns object code, which `neurc` passes to the system linker. `compile_to_ir` runs the same pipeline up to the optimizer and returns the module as text. The program's source text and path travel with the HIR so that panic messages can name `file:line:col`.

## Architecture

- **Dependencies**: `neuro-hir` (the HIR it consumes), `ast-types` (the operator enums), `shared-types`, `inkwell`, `thiserror`. The `syntax-parsing`, `hir-lowering` and `mlir-backend` crates are dev-dependencies only: tests and benches lower source before compiling, and a test that compiles a tensor operation links the body MLIR computes for it. inkwell is pinned to a commit of its master because no crates.io release supports LLVM 23 yet.
- **Public API**: `compile`, `compile_to_ir`, `ExternalBodies`, `BodyGuard`, `BodyGuardKind`, `BodyMemory`, `GpuVendor`, `OptimizationLevelSetting`, `CodegenError`, `CodegenResult`
- **Internals**: `pub(crate)` throughout. `codegen/` holds the emitter, split by construct (expressions, statements, drops, collections, and so on). `type_mapping.rs` and `types.rs` map Neuro types to LLVM types. `softfloat/` holds the half-precision builtins.
- **Output**: a platform object file, or the textual module from `compile_to_ir`.

### Module emission order

`compile` splits into `build_module` (generate and verify) and `emit_object_code`. Inside `build_module` the order is fixed:

1. Every function, method and closure signature is declared before any body, so a call resolves whatever the item order. Monomorphized instances rely on this.
2. Vtables are emitted for every `impl Trait for Type`.
3. Bodies are generated.
4. The standard-output drain is inserted on every exit path, if the module prints.
5. External bodies are linked, after every body, so each wrapper's declaration already exists.
6. Soft-float builtins are linked, if the module uses `half` or `bfloat`.

The module is then verified. The optimizer and instruction selection run after that (see [optimization levels](#optimization-levels)).

## Behavior

### Types and opaque pointers

LLVM 15 removed typed pointers, so every pointer is an opaque `ptr`. The backend keeps the Neuro type of each pointer beside it and passes that type to every load. `map_type` in [`type_mapping.rs`](../../../compiler/llvm-backend/src/type_mapping.rs) is the authority for the table below.

| Neuro type | LLVM type |
|---|---|
| `i8` to `i64`, `u8` to `u64` | `i8` to `i64` (signedness selects the instructions, not the type) |
| `f16`, `bf16` | `half`, `bfloat` |
| `f32`, `f64` | `float`, `double` |
| `bool` | `i1` |
| `char` | `i32` (a Unicode scalar value) |
| `string` | `{ ptr, i64 }` (see [Strings](#strings)) |
| `&string` | `{ ptr, i64 }` by value; `&mut string` is a `ptr` |
| `&[T]`, `&mut [T]` | `{ ptr, i64 }` by value (buffer pointer and length) |
| `&dyn Trait` | `{ ptr, ptr }` (data pointer and vtable pointer) |
| other `&T`, `&mut T` | `ptr` |
| `Tensor<T, S>` | `ptr` (a DLPack handle, see [Tensors](#tensors-and-dlpack)) |
| struct | anonymous struct, fields in declaration order |
| enum | `{ i32 tag, [W x [K x i64]] payload }` |
| tuple | anonymous struct, elements in position order |
| `[T; N]` | `[N x T]` |
| closure, `(A) -> B` | `{ ptr, ptr }` (function pointer and environment) |
| `Vec`, `HashMap`, `BTreeMap`, `StringBuilder` | `{ ptr, i64, i64, i64 }` header (see [Collections](#collections-and-builtins)) |
| newtype | its inner type (erased) |
| `void` | no value |

A bare `dyn Trait` or `[T]` is unsized and is rejected. Only the reference form maps.

### Strings

A `string` is `{ ptr, i64 }`: the pointer to UTF-8 bytes, and the byte count without the terminator. Literals live in `.rodata`, and the NUL after them exists only for C-string validity. The length is authoritative, so interior NUL bytes are ordinary content. A string is passed and returned by value, and an immutable `&string` is the same fat pointer, not a pointer to one, so a computed slice needs no stack slot.

`==` and `!=` compare the length, then call libc `memcmp`. `+` allocates a new buffer with no terminator.

Whether a buffer must be freed is decided at compile time, never read from the value: a literal and a heap string have the same shape. `produces_owned_string` in [`drops/owned_strings.rs`](../../../compiler/llvm-backend/src/codegen/drops/owned_strings.rs) answers for each expression, and the answer is conservative on purpose. A missed "owned" leaks a buffer. A wrong "owned" hands `.rodata` to `free`. The full ownership rules are in the [CONTEXT.md](../../../compiler/llvm-backend/CONTEXT.md).

### Structs, enums and methods

Structs are anonymous LLVM structs and are passed by value, with no `sret`. Values live in `alloca` slots, and a field read is a `getelementptr` plus `load`. A functional update such as `Point { x: 1.0, ..p }` seeds the aggregate from the base value.

An `impl` method is a free function named `TypeName__methodName`. `codegen_method_call` recovers the receiver type by splitting the symbol on `__`, so the separator must appear exactly once. Two rules guarantee that. Semantic analysis rejects a declared name containing `__`, and a monomorphized instance carries a `$g` marker that no identifier can contain (`identity$g_i32`).

- `&self` and `self` pass the struct by value as the first parameter.
- `&mut self` passes a pointer, so field writes reach the caller's storage.
- An associated function has no receiver parameter, and `TypeName::func(args)` calls `TypeName__func`.

An overloaded operator needs no backend code: [HIR lowering](hir-lowering.md) turns it into an ordinary method call.

Enums are tagged unions. A payload field is written into a zeroed `[K x i64]` slot through memory, so any sized type round-trips bit for bit. `Some` and `None` resolve their tags by name, not by declaration order.

### Tensors and DLPack

A tensor value is a pointer to a `DLManagedTensorVersioned` (DLPack 1.1). A foreign consumer takes the same pointer, so there is no wrap step at an FFI boundary. The handle's `data` field addresses a flat, row-major element buffer, 64-byte aligned, with strides counted in elements. A freshly built tensor has `device` `kDLCPU`. It becomes `kDLCUDA` (or `kDLROCM` for an AMD build) after `.to(Device::GPU(n))` or a device-side call.

The backend emits the operations that have no loop: building a handle, element read and write (`t[i, j]`, bounds-checked in every build), `.clone()`, `.to(...)`, `.detach()`, `.reshape(...)` and `.flatten(...)` (which rewrite the handle's shape without copying), and the derivative slots `.grad()`, `.hessian()` and `.zero_grad()`. Every host read of an existing tensor goes through `load_host_data`, which aborts with a located diagnostic instead of dereferencing device memory.

Releasing a tensor calls the handle's own `deleter`, so scope exit and a foreign owner run the same code. `__neuro_dlpack_deleter` frees the gradient and Hessian slots, then the buffer, then the structure. The structure and its control block share one `malloc`. The buffer is a separate allocation, because DLPack needs a 64-byte alignment that `malloc` does not promise. On Windows the buffer comes from `_aligned_malloc`, which only `_aligned_free` can release. The [CONTEXT.md](../../../compiler/llvm-backend/CONTEXT.md) covers the layout in full.

A tensor held in a struct field is not freed by the struct's drop, the same gap collection fields have.

### Integer arithmetic

The overflow rule follows the optimization level (see the [CLI help](../../getting-started/installation.md) for the user-facing wording):

- **`-O0`**: `+`, `-`, `*`, unary `-` and shifts are checked. A failure prints a located panic and aborts.
- **`-O1` and up**: arithmetic wraps (two's complement). The checks are not emitted.

Zero divisors are checked in every build, because there is no two's-complement answer to wrap to. `MIN / -1` follows the overflow rule. A `const` never wraps: an initializer that overflows its type is a compile error (`constant expression overflows`), not a silent value.

`.wrapping_*`, `.saturating_*`, `.checked_*`, `.abs()` and `.to_checked::<T>()` are intrinsics on the receiver type. Float-to-integer `as` casts saturate, and NaN maps to zero, so no cast yields `poison`.

### Control flow and stack slots

Every local, parameter, result slot and induction variable is an `alloca` in the entry block, placed by `CodegenContext::entry_alloca`. A slot allocated inside a loop body would grow the stack on every iteration, so entry placement keeps long loops from exhausting the stack. `mem2reg` promotes these slots to registers from `-O1` up, and it only promotes entry-block allocas.

`if` and `match` branch into merge blocks, and a value-producing form writes a result slot that is loaded at the merge. `match` evaluates its scrutinee once and tests the arms in order. `for` and `loop` branch to a dedicated step or body block so `continue` reaches the right place, and a `break v` stores its value into the loop's slot. `&&` and `||` short-circuit through a `phi`. The fall-through of a `match` is `unreachable`, because exhaustiveness is checked upstream.

### Optimization levels

`OptimizationLevelSetting` picks two things. It sets the `TargetMachine` level, which governs instruction selection, and it runs the IR pass pipeline over the verified module. The `TargetMachine` level alone runs no IR passes, so an `alloca` would never be promoted and nothing would inline.

| Setting | `TargetMachine` | IR pipeline | Integer overflow |
|---|---|---|---|
| `O0` | none | none | checked (panics) |
| `O1` | less | `default<O1>` | wraps |
| `O2` | default | `default<O2>` | wraps |
| `O3` | aggressive | `default<O3>` | wraps |

`-O0` runs no pipeline, which keeps its checks and guards exactly where codegen emitted them. Before a module is printed or compiled, its triple and data layout are set, so the optimizer reasons about the real size and alignment of each type.

### Panics and error-path outlining

Panic-family builtins (`panic`, `assert`, `unreachable`) and every bounds or UTF-8 guard abort. There is no unwinding and no landing pad, so the happy path costs nothing, and `Drop` runs only on normal scope exit.

The failure path is emitted into a module-private cold function, a thunk named `neuro.cold.panic.N` with attributes `cold noreturn noinline minsize`. The call site keeps the `cold noreturn` attributes, so they survive inlining of the enclosing function. `noinline` is required: without it the inliner folds a single-call-site thunk straight back in. Each guard branch carries `!prof` weights (`2000 : 1`), so block placement keeps the failure edge off the fall-through path. Thunks are deduplicated by their rendered text, so copies of one generic body share one thunk. A `panic` with a runtime message passes the message as a `(ptr, i64)` pair.

A panic writes its message to stderr with `write(2, ...)`, then calls `abort()`, which ends the process with status 134 on Linux.

Compiling a file `add.nr` that holds this function, at `-O0` with `neurc compile add.nr --emit llvm-ir`, gives the IR below. The panic text names that file:

```neuro
func add(a: i32, b: i32) -> i32 {
    return a + b
}
```

The output also contains the prelude's functions, and the `write` and `abort` declarations, which are omitted here:

```llvm
@panic.str.4 = private unnamed_addr constant [40 x i8] c"panic: integer overflow at add.nr:2:12\0A\00"

define i32 @add(i32 %0, i32 %1) {
entry:
  %a = alloca i32, align 4
  store i32 %0, ptr %a, align 4
  %b = alloca i32, align 4
  store i32 %1, ptr %b, align 4
  %a1 = load i32, ptr %a, align 4
  %b2 = load i32, ptr %b, align 4
  %addtmp = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %a1, i32 %b2)
  %arith.res = extractvalue { i32, i1 } %addtmp, 0
  %arith.ovf = extractvalue { i32, i1 } %addtmp, 1
  %arith.ok = xor i1 %arith.ovf, true
  br i1 %arith.ok, label %guard.cont, label %guard.fail, !prof !0

guard.fail:                                       ; preds = %entry
  call void @neuro.cold.panic.2() #1
  unreachable

guard.cont:                                       ; preds = %entry
  ret i32 %arith.res
}

; Function Attrs: cold minsize noinline noreturn
define private void @neuro.cold.panic.2() #0 {
entry:
  %panic.write = call i64 @write(i32 2, ptr @panic.str.4, i64 39)
  call void @abort()
  unreachable
}

declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32) #2

attributes #0 = { cold minsize noinline noreturn }
attributes #1 = { cold noreturn }
attributes #2 = { nocallback nocreateundeforpoison nofree nosync nounwind speculatable willreturn memory(none) }
!0 = !{!"branch_weights", i32 2000, i32 1}
```

Here the `+` is overflow-checked, so it calls `llvm.sadd.with.overflow` and branches on the overflow bit to the outlined thunk. With `-O2`, the same function is a single `add`.

### Standard output

`print` and `println` write into a 4096-byte buffer in `.bss`. A write that does not fit drains the buffer first. A write larger than the buffer goes straight to fd 1, so one huge string stays one syscall. `println` flushes at its newline only when fd 1 is a terminal (`isatty`, resolved once), so interactive output stays line by line while a pipe or file gets full buffers. `print` writes no terminator and never flushes at its end.

The buffer must reach fd 1 before the process stops. `main` returns and the panic `abort` are the only exits, so `finalize_stdout_buffer` inserts a drain before every `ret` in `main` and before every recorded abort. The drain sits in front of the panic call too, so the diagnostic on stderr follows the stdout that led up to it.

### Closures and dynamic dispatch

A closure value is `{ fn_ptr, env_ptr }`. The environment is a `malloc`'d, reference-counted block, shared by every copy of the value, and it is null when nothing is captured. The closure function takes the environment as its first parameter, and a call through a local variable is an indirect call. `__neuro_fn_retain` and `__neuro_fn_release` manage the count.

A `&dyn Trait` is `{ data, vtable }`. Each `impl Trait for Type` gets a constant `[N x ptr]` vtable in the trait's declaration order. Its slots are thunks: a `&self` method takes its struct by value, so the thunk loads the receiver first. A derived or builtin implementor exposes its trait methods under a `<method>$dyn` name, which the vtable uses.

### Drops and the pool arena

Drop types run `{struct}__drop(&mut self)` at each scope exit. Every owned binding has an `i1` drop flag, and every move clears it, so a moved value is dropped once. Holders keep one flag per owning field or element, so a partial move releases the rest. Assignment releases the displaced value after the new one is built, because the new value may read the old one.

A `pool { }` block allocates from a bump arena, `__neuro_arena_base` and `__neuro_arena_offset`. The block releases everything at once by restoring a mark. Which allocations use the arena is decided at compile time by `pool_depth`. An allocation inside a callee stays on the heap, and a store to a place that outlives the block is routed to libc. `PoolAware` values register with the pool and get a LIFO sweep at exit, in place of their `Drop`. A release goes through `__neuro_release`, which ignores pointers inside the arena, so one wrapper serves both allocators.

### Collections and builtins

`Vec<T>`, `HashMap<K, V>`, `BTreeMap<K, V>` and `StringBuilder` share the `{ buffer, len, cap, used }` header, held by value in the owner's slot. Elements live in one heap buffer.

- `Vec<T>` doubles its capacity, with a minimum of 8. `v[i]` is bounds-checked in every build.
- `HashMap<K, V>` is an open-addressed table with linear probing and tombstones, rehashed at a 3/4 load factor.
- `BTreeMap<K, V>` is a sorted array with binary search, so iteration is ordered.
- `StringBuilder` is a byte run. Its `to_string` copies out a new `string`.

Key equality, order and hash are supplied for integers, `bool` and `string` (FNV-1a for strings, SplitMix64 for integers). Struct keys use their `{Struct}__eq`, `__lt` and `__hash` methods, or the derived versions.

String `slice` and `char_slice` compute a borrowed fat pointer, with bounds and UTF-8 boundary checks. `char_at` is the only byte-indexed string read, and the prelude's codepoint iterator is its one caller.

### MLIR bodies

This backend has no loops over a tensor's elements. Each such operation reaches it as a call to a function that [HIR lowering](hir-lowering.md) outlined, and whose body the [MLIR backend](mlir-backend.md) computed. `neurc` passes those bodies as LLVM IR text plus the `(function, symbol)` pairs. A call to one becomes a call to its symbol, and the backend loads each tensor's buffer from its handle and passes it as an exploded row-major `memref` descriptor. It allocates the result tensor itself and passes that buffer as one more descriptor, then releases the tensors the operation took by value.

The linked IR is parsed into the module's own context and optimized at `-O2` whatever the program's level. Nothing in it permits reassociating floats, so the bits do not change. Each linked symbol is made internal so the optimizer can inline it. In a program that moves no tensor to a device, the call is emitted where it is made, so the result is the caller's own allocation, and a `pool` body's operations allocate from its arena.

A body with integer checks takes an `i64` status word as one more descriptor, before its result. The wrapper fills it with all ones, and after the call reads the failed check's number from its low 12 bits. It then raises the panic that the operator would raise here (`integer overflow`, `division by zero`, `remainder by zero`, `shift overflow` or `range step must be positive`), at the operator's offset.

### GPU runtime

A module with device code links the backend's GPU runtime. It defines MLIR's `mgpu*` ABI over the CUDA driver (`libcuda.so.1`) or over HIP (`libamdhip64.so`, for a `GpuVendor::Amd` build, which `--gpu-arch gfxNNN` selects). Both are generated from one C source, `gpu_runtime.c`, with a vendor block for the calls that differ. The library is opened with `dlopen` on first use, so the binary does not depend on the driver and a missing GPU becomes a diagnostic.

- **Staging.** A `Device` body's wrapper copies each host tensor operand to the device, runs the kernels, and copies the result back into the host tensor it returns. A device-resident operand is passed as it is, and the result is left on the device. An all-host call behaves as before.
- **Arena.** Each device has a 64 MiB linear arena for staging and launcher scratch. A request that does not fit spills to the driver's allocator.
- **Devices.** Each device has its own context, stream, arena and kernel modules. One device is current at a time, and a staged call switches to its operands' device and back.
- **Startup.** Module loads run in a global constructor, so a program with a `@gpu` function checks for a usable GPU before `main`. Every runtime failure is an ordinary `panic:`.
- **Fallback.** A `@gpu(fallback: true)` function is emitted three times: `f.gpu`, the staging wrapper; `f.host`, the host body; and `f`, which calls `__neuro_gpu_usable` to pick one. The constant `__neuro_gpu_fallback` is 1 when the program has no bare `@gpu` function. Then a load with no usable GPU leaves the module unloaded, and every call takes the host body.
- **Outlined operations.** A tensor operation that follows its operands emits the same three functions. `f` picks `f.gpu` when any tensor operand lives on a GPU, and `f.host` otherwise.

Element reads and writes and `.clone()` need no kernel. They branch on the tensor's device where they stand, so they work on a device tensor in any build.

### Soft-float builtins

On generic x86-64, LLVM lowers `half` and `bfloat` conversions to runtime calls such as `__extendhfsf2` and `__truncsfhf2`. Linux and macOS get them from libgcc or compiler-rt. The Windows linkers provide none, so `src/softfloat/` defines them. They are `weak_odr` (a platform runtime may override them) and integer-only, so they never call back into these conversions. `compile` links them only when the module uses `half` or `bfloat`. [`builtins.ll`](../../../compiler/llvm-backend/src/softfloat/builtins.ll) is generated from `reference.c` and was checked exhaustively against clang's native `_Float16` and `__bf16`.

## Diagnostics

### Runtime panics

These are what a program prints when a check fails. Each comes from the `panic:` path (or `assertion failed`, or `internal error`), with the `file:line:col` of the operation. The rows say which builds check it.

| Message | Cause | Builds |
|---|---|---|
| `panic: integer overflow at …` | `+`, `-`, `*`, unary `-`, or `MIN / -1` overflows | `-O0` only |
| `panic: shift overflow at …` | a shift amount is outside `0..width` | `-O0` only (masked above) |
| `panic: division by zero at …` | `/` by zero | every build |
| `panic: remainder by zero at …` | `%` by zero | every build |
| `panic: array index out of bounds at …` | an array, `Vec`, slice or tensor index outside its length | every build |
| `panic: string slice splits a UTF-8 code point at …` | a byte `slice` endpoint inside a code point | every build |
| `panic: range step must be positive at …` | `.step(n)` with `n <= 0` | every build |
| `panic: <message> at …` | `panic(msg)` | every build |
| `assertion failed at …` | `assert(cond)` is false | every build |
| `internal error: entered unreachable code at …` | `unreachable()` runs | every build |

GPU failures use the same `panic:` path. When no GPU is usable, the message reads `<what> needs an NVIDIA GPU, and none is usable: <reason>` (`AMD` for an AMD build), where `<what>` is `@gpu` or `@kernel`.

### Compile-time errors

`CodegenError` covers what codegen cannot emit. Most variants are internal invariant breaks, because the type checker has already rejected invalid source, so a user rarely sees one. A few are reachable from source. A `const` whose value overflows its type reports `constant expression overflows <type>`. The variants are listed in [`errors.rs`](../../../compiler/llvm-backend/src/errors.rs). An LLVM builder failure surfaces as `LLVM error: …`.

## Testing

- Unit tests sit beside the code in `src/`, including [`tests.rs`](../../../compiler/llvm-backend/src/tests.rs) and the `#[cfg(test)]` modules of `type_mapping.rs` and `codegen/`.
- Benchmarks are in [`benches/`](../../../compiler/llvm-backend/benches/).
- End-to-end behavior, including every panic message above, is covered by the `neurc` suites in [`compiler/neurc/tests/suite/`](../../../compiler/neurc/tests/suite/).

Run the crate's tests with the three prefix variables set to one LLVM and MLIR 23 install, as [Installation](../../getting-started/installation.md#mlir) describes:

```bash
cargo test -p llvm-backend
```

## Source

- [`compiler/llvm-backend/src/`](../../../compiler/llvm-backend/src/)
- [`compiler/llvm-backend/CONTEXT.md`](../../../compiler/llvm-backend/CONTEXT.md): the authoritative ABI contract, with the rationale behind each rule

## See Also

- [MLIR Backend](mlir-backend.md), which computes the tensor loops this backend links
- [HIR Lowering](hir-lowering.md), the stage before this one
- [Language reference: types](../../language-reference/types.md), [memory model](../../language-reference/memory-model.md) and [tensors](../../language-reference/tensors.md)
- [inkwell](https://github.com/TheDan64/inkwell) and the [LLVM Language Reference](https://llvm.org/docs/LangRef.html)
