# LLVM Backend

**Crate**: `compiler/llvm-backend`
**Library**: inkwell 0.10.0 (LLVM 23 bindings)
**Build requirement**: `LLVM_SYS_231_PREFIX` pointing at an LLVM 23 install

## Overview

The LLVM backend generates native object code from the typed High-Level IR (`neuro-hir`), not
the AST. [HIR lowering](hir-lowering.md) has already attached a resolved type to every
expression, so the backend reads types inline instead of re-deriving them. It uses
[inkwell](https://github.com/TheDan64/inkwell) (safe Rust bindings to LLVM 23) to produce
optimized machine code for the host platform.

**Entry points:**
```rust
pub fn compile(program: &HirProgram, optimization: OptimizationLevelSetting,
               source: &str, source_path: &str,
               external: &[ExternalBodies]) -> CodegenResult<Vec<u8>>
pub fn compile_to_ir(program: &HirProgram, optimization: OptimizationLevelSetting,
                     source: &str, source_path: &str,
                     external: &[ExternalBodies]) -> CodegenResult<String>
```

`compile` returns object code; `compile_to_ir` stops after the pass pipeline and returns the
textual module (`neurc compile --emit llvm-ir`). `source` / `source_path` are carried through for
located runtime-panic diagnostics (array bounds, slice boundaries, integer overflow).
`external` names function bodies another backend computed; see [MLIR bodies](#mlir-bodies).

## Architecture

- **Dependencies**: `neuro-hir` (the typed HIR it consumes), `ast-types`, `shared-types`, `inkwell 0.10.0`, `thiserror`; `syntax-parsing` and `hir-lowering` are dev-dependencies (tests and benches lower before compiling)
- **Public API**: `compile`, `compile_to_ir`, `ExternalBodies`, `OptimizationLevelSetting`, `CodegenError`
- **All internals**: `pub(crate)`, `CodegenContext`, `TypeMapper`, `codegen_*` helpers
- **Output**: platform object code (`.o`) passed to the system linker by `neurc`

## Supported Features

### Types

| Neuro | LLVM |
|---|---|
| `i8` / `i16` / `i32` / `i64` | `i8` / `i16` / `i32` / `i64` |
| `u8` / `u16` / `u32` / `u64` | the same widths, with unsigned instruction selection |
| `f16` / `bf16` | `half` / `bfloat` (conversions via the soft-float builtins) |
| `f32` / `f64` | `float` / `double` |
| `bool` | `i1` |
| `char` | `i32` (a Unicode scalar value) |
| `string` | anonymous struct `{ ptr, i64 }`, a fat pointer: data pointer + byte length |
| `&string` | the `{ ptr, i64 }` fat pointer **by value**; `&mut string` is the referent's address |
| `&T` / `&mut T` (other `T`) | `ptr` (opaque) |
| `&dyn Trait` | `{ data ptr, vtable ptr }` fat pointer |
| user struct | anonymous LLVM struct `{ T0, T1, ... }`, fields in declaration order |
| tuple | anonymous LLVM struct, elements in position order |
| `[T; N]` | `[N x T]` |
| enum | tagged union `{ i32 tag, [W x [K x i64]] payload }`, `W` slots for the widest variant, `K` words for the widest payload field |
| closure | `{ fn_ptr, env_ptr }` fat pointer, no heap allocation |
| `void` | `void` |

The full ABI contract (string, struct, method, builtin-method, overflow, panic, drop,
collections, constants, and soft-float) is documented in the slice's
[CONTEXT.md](../../../compiler/llvm-backend/CONTEXT.md), which is authoritative.

### Expressions

Literals; variable loads; arithmetic, comparison, logical, and bitwise operators; unary
`-`, `!`, `~`; calls (free functions, associated functions, methods, and indirect calls
through a closure or a vtable); struct, enum, tuple, and array literals; field, index, and
tuple-element access; casts; ranges (as `.slice` arguments and `for` bounds); `if`, `match`,
`loop`, and block expressions in value position; and the `?` / `??` fallible operators,
both of which are already desugared to a `match` in HIR.

### Statements

- Bindings (`val`, `mut`), `alloca` in the **entry block**, never inside a loop
- Assignment to a place: a binding, a field, an element, a tensor coordinate, or a referent
- `return`, explicit and as a block's trailing expression
- `if` / `else`, basic blocks with a merge block; a branch that returns contributes no
  edge to the merge
- `while`, `for` (a dedicated step block, so `continue` advances the induction variable),
  and `loop` (whose value comes from its `break v` edges)
- `break` / `continue`, including labelled forms; these branch to the loop's exit / step block
- Scope-exit `Drop` calls and collection frees

### Signedness

Integer instructions are selected based on signedness:
- Signed: `sdiv`, `srem`, `icmp slt/sgt/sle/sge`
- Unsigned: `udiv`, `urem`, `icmp ult/ugt/ule/uge`

## Code Generation Pipeline

```text
build_module
  1. Declare every function, method and closure signature before any body,
     so a call resolves regardless of item order (monomorphized instances included)
  2. Emit vtables for every `impl Trait for Type`
  3. Generate bodies
  4. Insert the standard-output drain on every exit path, if the module prints
  5. Link the external bodies, if any were handed in
  6. Link the soft-float builtins, if the module uses `half` / `bfloat`
  7. Verify the module
then
  8. Create a target machine for the host triple and run the `-O` pass pipeline
  9. Emit object code to a memory buffer (`compile`) or print the module (`compile_to_ir`)
```

Why the order is fixed is recorded under *Module Emission Order* in the slice's
[CONTEXT.md](../../../compiler/llvm-backend/CONTEXT.md).

## Opaque Pointers (LLVM 15+)

LLVM 15 removed typed pointers. All pointers are now opaque (`ptr`). The backend tracks the Neuro type alongside every pointer in `variable_types: HashMap<String, BasicTypeEnum>` and supplies the type explicitly to every `build_load()` call.

## String ABI

`string` values are represented as an anonymous LLVM struct `{ ptr, i64 }`:
- **field 0** (`ptr`): pointer to the UTF-8 bytes, followed by a NUL (in `.rodata` for a literal, on the heap for a built string)
- **field 1** (`i64`): byte count excluding the null terminator

The fat pointer is passed and returned by value. On x86-64 SysV this fits in two registers (no sret needed). `==` and `!=` lower to a length check followed by a `memcmp` against an external libc symbol; a `select` passes `n=0` to `memcmp` when lengths differ, keeping it safe.

## Struct and Method ABI

User-defined structs are lowered to anonymous LLVM struct types `{ T0, T1, ... }` with fields in declaration order. All struct values are stack-allocated via `alloca`; field reads use `getelementptr` + `load`, field writes use `getelementptr` + `store`.

`impl` methods are lowered to free functions with a mangled name `StructName__methodName`. For `&self` instance methods the struct is passed by value as the first LLVM parameter (`self`). Associated functions (no `self`) have no implicit first parameter and are called via `TypeName::func(args)`.

## Error-Path Outlining

Every panic-family failure path (`panic` / `assert` / `unreachable`, the array and `Vec`
bounds guards, and the string-slice bounds and UTF-8 boundary checks) is emitted into a
module-private cold function rather than inline in the function that can fail:

```llvm
guard.fail:
  call void @neuro.cold.panic.0() #1   ; cold noreturn
  unreachable

; Function Attrs: cold minsize noinline noreturn
define private void @neuro.cold.panic.0() #0 {
entry:
  %panic.write = call i64 @write(i32 2, ptr @panic.str, i64 46)
  call void @abort()
  unreachable
}
```

The diagnostic machinery (one `write(2, …)` per message fragment, plus the `abort()`) would
otherwise occupy cache lines between the guard branch and the code that follows it, at
every check. `noinline` is what holds the split in place; without it the inliner folds a
single-call-site function straight back in. Thunks are deduplicated by their rendered
diagnostic text, so the copies monomorphization makes of one generic body share a single
thunk.

A `panic(msg)` whose message is a runtime `string` uses a `(ptr, i64)` thunk: only the
constant fragments are baked in, and the fat pointer travels as two arguments.

Each guard branch also carries `!prof` branch weights (`2000 : 1`) marking the failure edge
as the improbable one, so block placement keeps it off the fall-through path. At `-O0` the
integer-overflow check is one more guard of the same shape: it prints
`panic: integer overflow at file:line:col` through an outlined thunk. From `-O1` up, integer
arithmetic wraps and carries no check.

## Error Types

Codegen reports a `CodegenError`. Most variants describe an internal invariant break (an
unsupported type reaching the backend, an LLVM builder failure) rather than a fault in the
program, because the type checker has already rejected invalid source. The authoritative list is
[`compiler/llvm-backend/src/errors.rs`](../../../compiler/llvm-backend/src/errors.rs).

## Usage

```rust
use syntax_parsing::parse;
use hir_lowering::lower_program;
use llvm_backend::{compile, OptimizationLevelSetting};

let source = r#"
    func add(a: i32, b: i32) -> i32 {
        a + b
    }
"#;

let ast = parse(source)?;
let hir = lower_program(&ast)?;                  // hir-lowering: AST → typed HIR
let object_code = compile(&hir, OptimizationLevelSetting::O2, source, "add.nr", None)?;
std::fs::write("output.o", &object_code)?;
```

This skips the stages a real program needs between parsing and lowering (module resolution,
argument binding, type checking); `compile_file` in `neurc` runs them all.

## LLVM IR Example

**Neuro source:**
```neuro
func add(a: i32, b: i32) -> i32 {
    return a + b
}
```

**Generated LLVM IR at `-O0`** (`neurc compile --emit llvm-ir`, alignment annotations
dropped). The `+` is overflow-checked at this level, so the add goes through
`llvm.sadd.with.overflow` and a guard branch to an outlined panic thunk:
```llvm
define i32 @add(i32 %0, i32 %1) {
entry:
  %a = alloca i32
  store i32 %0, ptr %a
  %b = alloca i32
  store i32 %1, ptr %b
  %a1 = load i32, ptr %a
  %b2 = load i32, ptr %b
  %addtmp = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %a1, i32 %b2)
  %arith.res = extractvalue { i32, i1 } %addtmp, 0
  %arith.ovf = extractvalue { i32, i1 } %addtmp, 1
  %arith.ok = xor i1 %arith.ovf, true
  br i1 %arith.ok, label %guard.cont, label %guard.fail, !prof !0

guard.fail:
  call void @neuro.cold.panic.0() #1
  unreachable

guard.cont:
  ret i32 %arith.res
}
```

## Testing

The crate's tests cover, at a glance: primitive type mapping, the signedness/float type
predicates, compiling a simple arithmetic function to non-empty object code, a multi-function
program with variable declarations and calls, and `OptimizationLevelSetting::from_u8` (accepts
0 to 3, rejects anything higher). The full list lives beside the source in
[`compiler/llvm-backend/src/`](../../../compiler/llvm-backend/src/).

Run with:
```bash
LLVM_SYS_231_PREFIX=/usr cargo test -p llvm-backend
```

## Design Decisions

### Why inkwell?

inkwell provides safe, type-checked Rust bindings to the LLVM C API. The alternative, calling `llvm-sys` (raw unsafe bindings) directly, would require manual lifetime management and is significantly more error-prone. inkwell compiles against the exact LLVM version specified by the feature flag (`llvm23-1`), preventing version mismatch at link time.

### Stack Allocation for All Locals

All local variables and parameters are stack-allocated via `alloca` in the entry block. This
is correct and simple, and LLVM's `mem2reg` pass (enabled from `-O1`) promotes them to SSA
registers during optimization.

### Optimization Levels

The `OptimizationLevelSetting` enum maps to LLVM's optimization levels:

| Setting | LLVM | Use |
|---|---|---|
| `O0` | None | Debugging, preserves all allocas |
| `O1` | Less | Light optimization + mem2reg |
| `O2` | Default | Standard release build |
| `O3` | Aggressive | Maximum optimization |

## MLIR bodies

`neurc` hands this backend the tensor bodies the
[MLIR backend](mlir-backend.md) computed, as LLVM IR text plus the function each symbol stands
for. Each named function is still declared and defined here, with its ordinary tensor ABI, but
its body is a call: the backend loads each tensor's buffer out of its DLPack handle and passes it
as an exploded row-major `memref` descriptor, allocates the result tensor itself and passes that
buffer as one more descriptor, then releases every tensor the function took by value. The IR is
parsed into the module's own context and linked in after every body, and each linked symbol is
made internal so the optimizer can inline it. inkwell stays the terminal code-emission layer on
every path.

Bodies marked `BodyMemory::Device` launch GPU kernels, which read and write device memory only,
so the wrapper stages them. It copies each tensor operand into a device buffer, hands the kernels
a device buffer for the result, copies the result back into the host tensor it returns, and
releases the staged buffers before returning. Callers still pass and receive host tensors. Device
buffers come from a second linear arena, the `pool` arena's rules over a 64 MiB chunk of device
memory: one call's staging is one region released by a single mark restore, and a buffer that
does not fit spills to the GPU runtime's allocator. The same allocator serves the buffers a
launcher allocates for itself between two kernels. In a program with device bodies, each `pool`
block marks the device arena on entry and restores it at exit, after its `PoolAware` sweep: one
batched release per device.

`external` is a list, one set of bodies per memory kind, so a program's `@gpu` bodies and its
host MLIR bodies link side by side. A module with device bodies also links the backend's own GPU
runtime: MLIR's `mgpu*` ABI implemented over the CUDA driver, or over HIP when `compile` is given
`GpuVendor::Amd`, which it opens with `dlopen` on first use rather than linking against it. An
AMD build's device tensors report `kDLROCM` over DLPack instead of `kDLCUDA`. The launchers load their kernels from a global constructor,
so a program checks for a usable GPU before `main`, and every runtime failure, a missing GPU
included, is an ordinary `panic:` that drains buffered output and aborts.

A `@kernel` function's wrapper stages the same way but has no result: it returns nothing, and
each `&mut` tensor it was handed is staged like an operand, then copied back over the host
buffer once the kernel has run. A `&mut` tensor already on the device is written in place. A
program with a `@kernel` function, like one with a bare `@gpu` function, has no host body to
fall back on, so a missing GPU is fatal at startup.

A `@gpu(fallback: true)` function is emitted three times: `f.gpu`, the staging wrapper above;
`f.host`, the backend's own body; and `f` itself, which asks the runtime's
`__neuro_gpu_usable` which of the two to call. The backend also defines the constant
`__neuro_gpu_fallback`, 1 when no bare `@gpu` function exists. With it set, a module load
that finds no usable GPU leaves the module unloaded instead of aborting, and every call takes
its host body.

A tensor operation lowering outlined to follow its operands (`HirTarget::FollowsOperands`) is
emitted the same three ways, but `f` picks per call: `f.gpu` when any tensor operand lives on a
GPU, `f.host` when every one is a host tensor. Element reads and writes and `.clone()` need no
kernel. They branch on the tensor's DLPack device where they stand, copying one element through
the runtime or cloning the buffer on its GPU, so they work on a device tensor in any build.

## Source

- [`compiler/llvm-backend/src/`](../../../compiler/llvm-backend/src/)
- [`compiler/llvm-backend/CONTEXT.md`](../../../compiler/llvm-backend/CONTEXT.md)

## See Also

- [LLVM Language Reference](https://llvm.org/docs/LangRef.html)
- [inkwell Documentation](https://thedan64.github.io/inkwell/)
- [inkwell GitHub](https://github.com/TheDan64/inkwell)
- [LLVM Kaleidoscope Tutorial](https://llvm.org/docs/tutorial/MyFirstLanguageFrontend/index.html)
