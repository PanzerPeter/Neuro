# LLVM Backend

**Status**: Complete (Phase 1)
**Crate**: `compiler/llvm-backend`
**Library**: inkwell 0.9.0 (LLVM 20 bindings)
**Build requirement**: `LLVM_SYS_201_PREFIX=/usr/lib/llvm20`

## Overview

The LLVM backend slice generates native object code from the typed High-Level IR (`neuro-hir`), not the AST. Since 1D the frontend lowers the type-checked AST to HIR (`hir-lowering`), where every expression already carries its resolved type, so the backend reads types inline instead of re-deriving them. It uses [inkwell](https://github.com/TheDan64/inkwell) (safe Rust bindings to LLVM 20) to produce optimized machine code for the host platform.

**Entry point:**
```rust
pub fn compile(
    program: &HirProgram,
    optimization: OptimizationLevelSetting,
    source: &str,
    source_path: &str,
) -> CodegenResult<Vec<u8>>
```

`source` / `source_path` are carried through for located runtime-panic diagnostics (e.g. array
bounds, slice boundaries).

## Architecture

- **Dependencies**: `neuro-hir` (the typed HIR it consumes), `ast-types`, `shared-types`, `source-location`, `diagnostics`, `inkwell 0.9.0`; `hir-lowering` is a dev-dependency (tests/benches lower before compiling)
- **Public API**: single `compile()` function returning object code bytes
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
| `&T` / `&mut T` (other `T`) | `ptr` (opaque, LLVM 20) |
| `&dyn Trait` | `{ data ptr, vtable ptr }` fat pointer |
| user struct | anonymous LLVM struct `{ T0, T1, ... }`, fields in declaration order |
| tuple | anonymous LLVM struct, elements in position order |
| `[T; N]` | `[N x T]` |
| enum | tagged union `{ i32 tag, [W x i64] payload }`, `W` sized to the widest variant |
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
- Assignment, field / index / dereference assignment
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

```
1. Pre-pass: register struct definitions and extract all function/method signatures (including mangled method names `StructName__methodName`)
2. Initialize LLVM context + module (via inkwell)
3. Pre-pass: collect expression types for instruction selection
4. For each function:
   a. Create LLVM function with parameter types
   b. Allocate parameters on stack (alloca + store)
   c. Generate body statements
5. Verify LLVM module (catches malformed IR)
6. Initialize native target (LLVM_SYS_201_PREFIX)
7. Create target machine for the host triple
8. Emit object code to memory buffer
```

## Opaque Pointers (LLVM 15+)

LLVM 15 removed typed pointers. All pointers are now opaque (`ptr`). The backend tracks the Neuro type alongside every pointer in `variable_types: HashMap<String, BasicTypeEnum>` and supplies the type explicitly to every `build_load()` call.

## String ABI

`string` values are represented as an anonymous LLVM struct `{ ptr, i64 }`:
- **field 0** (`ptr`): pointer to null-terminated UTF-8 bytes in `.rodata`
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

The diagnostic machinery, one `write(2, …)` per message fragment plus the `abort()`
otherwise occupies cache lines between the guard branch and the code that follows it, at
every check. `noinline` is what holds the split in place; without it the inliner folds a
single-call-site function straight back in. Thunks are deduplicated by their rendered
diagnostic text, so the copies monomorphization makes of one generic body share a single
thunk.

A `panic(msg)` whose message is a runtime `string` uses a `(ptr, i64)` thunk: only the
constant fragments are baked in, and the fat pointer travels as two arguments.

Each guard branch also carries `!prof` branch weights (`2000 : 1`) marking the failure edge
as the improbable one, so block placement keeps it off the fall-through path. The `-O0`
integer-overflow check is weighted but *not* outlined, its trap block is a single
`llvm.trap`, so moving it behind a call would trade one instruction for another.

## Error Types

```rust
pub enum CodegenError {
    InitializationFailed(String),
    UnsupportedType(String),
    UndefinedVariable(String),
    UndefinedFunction(String),
    TypeMismatch { expected: String, found: String },
    InvalidOperandType { op: String, ty: String },
    InvalidOptimizationLevel(u8),
    LlvmError(String),
    MissingReturn,
    InternalError(String),
}
```

## Usage

```rust
use syntax_parsing::parse;
use semantic_analysis::type_check;
use llvm_backend::{compile, OptimizationLevelSetting};

let source = r#"
    func add(a: i32, b: i32) -> i32 {
        a + b
    }
"#;

let ast = parse(source)?;
type_check(&ast)?;
let hir = lower_program(&ast)?;                  // hir-lowering: AST → typed HIR
let object_code = compile(&hir, OptimizationLevelSetting::O2, source, "add.nr")?;
std::fs::write("output.o", &object_code)?;
```

## LLVM IR Example

**Neuro source:**
```neuro
func add(a: i32, b: i32) -> i32 {
    return a + b
}
```

**Generated LLVM IR (simplified, LLVM 20 opaque pointers):**
```llvm
define i32 @add(i32 %0, i32 %1) {
entry:
  %a = alloca i32
  %b = alloca i32
  store i32 %0, ptr %a
  store i32 %1, ptr %b
  %2 = load i32, ptr %a
  %3 = load i32, ptr %b
  %addtmp = add i32 %2, %3
  ret i32 %addtmp
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
LLVM_SYS_201_PREFIX=/usr/lib/llvm20 cargo test -p llvm-backend
```

## Design Decisions

### Why inkwell?

inkwell provides safe, type-checked Rust bindings to the LLVM C API. The alternative, calling `llvm-sys` (raw unsafe bindings) directly, would require manual lifetime management and is significantly more error-prone. inkwell compiles against the exact LLVM version specified by the feature flag (`llvm20-1`), preventing version mismatch at link time.

### Stack Allocation for All Locals

All local variables and parameters are stack-allocated via `alloca`. This is the standard approach for a non-optimized Phase 1 backend: it is correct, simple, and LLVM's `mem2reg` pass (enabled at `-O1`+) will promote them to SSA registers during optimization.

### Optimization Levels

The `OptimizationLevelSetting` enum maps to LLVM's optimization levels:

| Setting | LLVM | Use |
|---|---|---|
| `O0` | None | Debugging, preserves all allocas |
| `O1` | Less | Light optimization + mem2reg |
| `O2` | Default | Standard release build |
| `O3` | Aggressive | Maximum optimization |

## Future: MLIR Integration (Phase 2+)

`melior` (Rust MLIR bindings for LLVM/MLIR 20) is already integrated alongside inkwell in the
`mlir-backend` slice behind the off-by-default `mlir` feature (1D scaffold); both crates link
against the same LLVM 20 dylib via `LLVM_SYS_201_PREFIX`. When tensor types are introduced (Phase 2+)
that slice will lower the **same typed HIR** this backend consumes.

The planned lowering strategy:

```
typed HIR (neuro-hir)
  → MLIR dialects (linalg / tensor / func / arith)
  → Enzyme MLIR AD pass (@grad)
  → GPU dialects (nvgpu / rocdl / Triton)  or  llvm dialect
  → inkwell (final LLVM IR emission)
  → native object code
```

inkwell remains the terminal code-emission layer in all paths.

## Resources

- [LLVM Language Reference](https://llvm.org/docs/LangRef.html)
- [inkwell Documentation](https://thedan64.github.io/inkwell/)
- [inkwell GitHub](https://github.com/TheDan64/inkwell)
- [LLVM Kaleidoscope Tutorial](https://llvm.org/docs/tutorial/MyFirstLanguageFrontend/index.html)
- Source: [compiler/llvm-backend/src/](../../compiler/llvm-backend/src/)
