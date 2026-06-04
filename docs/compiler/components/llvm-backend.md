# LLVM Backend

**Status**: Complete (Phase 1)
**Crate**: `compiler/llvm-backend`
**Library**: inkwell 0.9.0 (LLVM 20 bindings)
**Build requirement**: `LLVM_SYS_201_PREFIX=/usr/lib/llvm20`

## Overview

The LLVM backend slice generates native object code from a type-checked AST. It uses [inkwell](https://github.com/TheDan64/inkwell) (safe Rust bindings to LLVM 20) to produce optimized machine code for the host platform.

**Entry point:**
```rust
pub fn compile(items: &[Item], optimization: OptimizationLevelSetting) -> CodegenResult<Vec<u8>>
```

## Architecture

- **Dependencies**: `ast-types`, `shared-types`, `diagnostics`, `inkwell 0.9.0`
- **Public API**: single `compile()` function returning object code bytes
- **All internals**: `pub(crate)` — `CodegenContext`, `TypeMapper`, `codegen_*` helpers
- **Output**: platform object code (`.o`) passed to the system linker by `neurc`

## Supported Features (Phase 1)

### Types

| Neuro | LLVM |
|---|---|
| `i8` | `i8` |
| `i16` | `i16` |
| `i32` | `i32` |
| `i64` | `i64` |
| `u8` | `i8` (unsigned semantics) |
| `u16` | `i16` (unsigned semantics) |
| `u32` | `i32` (unsigned semantics) |
| `u64` | `i64` (unsigned semantics) |
| `f32` | `float` |
| `f64` | `double` |
| `bool` | `i1` |
| `string` | anonymous struct `{ ptr, i64 }` (fat pointer: data ptr + byte length) |
| user struct | anonymous LLVM struct `{ T0, T1, ... }` with fields in declaration order |
| `void` | `void` |

### Expressions

- Integer, float, bool, and string literals
- Variable loads from stack
- Binary operators: arithmetic (`+`, `-`, `*`, `/`, `%`), comparison (`==`, `!=`, `<`, `>`, `<=`, `>=`), logical (`&&`, `||`)
- Unary operators: `-`, `!`
- Function calls (value-returning and void)
- Parenthesized expressions
- Struct literals — `alloca` + field-by-field `insertvalue`
- Field access — `getelementptr` + `load`
- Path expressions (`TypeName::func(args)`) — associated function calls

### Statements

- Variable declarations (`val`, `mut`) — stack-allocated via `alloca`
- Variable reassignment
- `return` statements (explicit and expression-based implicit returns)
- `if` / `else` — basic block management with merge blocks
- `while` loops — `while.cond` / `while.body` / `while.exit` blocks
- Range-for loops (`for i in start..end`) — dedicated step block so `continue` advances correctly
- `break` and `continue` — branch to loop exit/step blocks
- Field assignment (`obj.field = value`) — `getelementptr` + `store`

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
let object_code = compile(&ast, OptimizationLevelSetting::O2)?;
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

5 unit/integration tests:
1. `test_type_mapper_primitives` — all primitive types map without error
2. `test_type_predicates` — float/unsigned-int predicates
3. `test_compile_simple_function` — basic arithmetic function compiles to non-empty object code
4. `test_compile_milestone_program` — multi-function program with variable declarations and calls
5. `test_optimization_level_parsing` — `OptimizationLevelSetting::from_u8` accepts 0–3 and rejects 4+

Run with:
```bash
LLVM_SYS_201_PREFIX=/usr/lib/llvm20 cargo test -p llvm-backend
```

## Design Decisions

### Why inkwell?

inkwell provides safe, type-checked Rust bindings to the LLVM C API. The alternative — calling `llvm-sys` (raw unsafe bindings) directly — would require manual lifetime management and is significantly more error-prone. inkwell compiles against the exact LLVM version specified by the feature flag (`llvm20-1`), preventing version mismatch at link time.

### Stack Allocation for All Locals

All local variables and parameters are stack-allocated via `alloca`. This is the standard approach for a non-optimized Phase 1 backend: it is correct, simple, and LLVM's `mem2reg` pass (enabled at `-O1`+) will promote them to SSA registers during optimization.

### Optimization Levels

The `OptimizationLevelSetting` enum maps to LLVM's optimization levels:

| Setting | LLVM | Use |
|---|---|---|
| `O0` | None | Debugging — preserves all allocas |
| `O1` | Less | Light optimization + mem2reg |
| `O2` | Default | Standard release build |
| `O3` | Aggressive | Maximum optimization |

## Future: MLIR Integration (Phase 3+)

When tensor types are introduced, `melior` (Rust MLIR bindings for LLVM/MLIR 20) will be added alongside inkwell. Both crates link against the same LLVM 20 dylib via `LLVM_SYS_201_PREFIX`.

The planned lowering strategy:

```
AST → Neuro High-Level IR
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
