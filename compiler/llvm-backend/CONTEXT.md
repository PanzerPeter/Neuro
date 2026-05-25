# llvm-backend

## Purpose
Emit native object code from a type-checked Neuro AST via LLVM IR generation.

## Entry Point
- Type: Library function
- Input: `items: &[Item], optimization: OptimizationLevelSetting`
- Output: `Result<Vec<u8>, CodegenError>`

## Data Ownership
- Tables: none
- Events Published: none
- Events Consumed: none
- Public Read Model: none

## Shared Kernel
- ast-types — read-only traversal of the type-checked AST
- shared-types — type system primitives
- diagnostics — error type infrastructure

## Notes
inkwell 0.8.0 with feature `llvm20-1` (LLVM 20 bindings) is a third-party crate, not Neuro-owned Shared Kernel.
Requires LLVM 20 installed with MLIR enabled; set `LLVM_SYS_201_PREFIX` to the LLVM 20
prefix (e.g. `/usr/lib/llvm20`) before building.
`semantic-analysis` has no production dependency here; neurc orchestrates ordering so
that type checking always precedes code generation. `syntax-parsing` appears only in
`[dev-dependencies]` for integration tests.

## String ABI
`string` values are represented as an anonymous LLVM struct `{ ptr, i64 }`:
- field 0 (`ptr`): pointer to null-terminated UTF-8 bytes in read-only memory (`.rodata`)
- field 1 (`i64`): byte count of the string **excluding** the null terminator

The struct is passed and returned by value. On x86-64 SysV this fits in two registers
(rax/rdx or equivalent), so no sret indirection is needed for typical string functions.
The semantic type `Type::String` in `semantic-analysis` is unchanged; the fat pointer
layout is a backend implementation detail invisible to other slices.

`==` and `!=` on strings are lowered to a length check followed by a `memcmp` call
declared as an external libc symbol. `memcmp` is universally available on all supported
platforms (Linux, macOS). The length check uses `select` to pass `n=0` to `memcmp` when
lengths differ, keeping it safe without requiring additional basic blocks.

## Struct ABI
User-defined struct types are lowered to anonymous LLVM struct types `{ T0, T1, ... }`
with fields in declaration order (no padding insertion — LLVM handles natural alignment).
Struct values are stored on the stack via `alloca` and initialised field-by-field with
`insertvalue`. Field reads use `getelementptr` + `load`; field writes use
`getelementptr` + `store`. Struct types are not yet supported as function parameters
or return types (Phase 2+ limitation; the type mapper returns an error for those cases).
Field layout is held in `CodegenContext.struct_defs`; `get_struct_llvm_type` rebuilds
the anonymous LLVM struct type on demand (LLVM deduplicates by structure).

## Method ABI
`impl` methods are lowered to LLVM free functions under a mangled name
`StructName__methodName` (double-underscore separator). Users cannot define names
containing `__` because the identifier grammar forbids it.

For `&self` instance methods the struct is passed **by value** as the first LLVM
parameter (`param[0]`, named `self` in the alloca map). This is semantically correct
for read-only access; callers load their stack variable and pass the value directly.
`&mut self` and consuming `self` are rejected by semantic analysis before codegen runs.

Associated functions (no `self_param`) are lowered identically but have no implicit
first parameter; callers invoke them as `TypeName::func(args)` which the codegen maps
to `codegen_call("StructName__funcName", args)`.

Method calls (`instance.method(args)`) in `codegen_expr` are recognised when the `Call`
node's `func` is a `FieldAccess`. `fa_struct_names` (keyed by the `Call` span start)
carries the struct name so `codegen_method_call` can reconstruct the mangled name without
re-querying the AST.

## if / else-if / else Lowering

`codegen_if` lowers an `if/else if+/else?` chain by treating it as a binary tree:
- Each call creates three basic blocks — `then`, `else`, `ifcont` (merge).
- The `else` block either hosts the final `else` body directly or recursively calls
  `codegen_if` with the first remaining `else_if` arm, passing the rest of the arms
  and the original `else_block` to that recursive call.
- This `split_first` recursion ensures every arm is mutually exclusive with all
  subsequent arms; the final `else` body is only reachable when all preceding
  conditions are false.

## Future: MLIR Integration (Phase 3+)
When tensor operations are introduced, `melior` (Rust MLIR bindings, targeting the same
LLVM 20 / MLIR 20 installation) will be added alongside inkwell. The lowering strategy
will be: AST → Neuro High-Level IR → MLIR dialects (linalg/tensor/func/arith) →
Enzyme MLIR AD pass → GPU dialects (nvgpu/rocdl) or `llvm` dialect → inkwell for final
LLVM IR emission. inkwell remains the terminal code-emission layer in all paths.

## Constant Declarations ABI
Module-level consts are emitted as `@NAME = internal constant TYPE VALUE` LLVM globals before
any function definitions. Their LLVM value is also stored in `CodegenContext.const_values` so
that identifier references inside function bodies resolve without loading from the global.

Function-body `Stmt::Const` nodes fold the constant expression in Rust (via `FoldedConst`) and
store the resulting `BasicValueEnum` in `const_values` for the duration of the function scope.
No `alloca` is emitted; consts are purely compile-time values.

Constant folding uses a pure Rust `FoldedConst { Int(i64), Float(f64), Bool(bool), Str(String) }`
enum rather than inkwell's const-arithmetic API (which has inconsistent availability across
inkwell versions). All arithmetic is performed in Rust with wrapping semantics for integers and
IEEE-754 for floats; a single `const_int`/`const_float`/`const_struct` call creates the final
LLVM value.

`global_const_types: HashMap<String, Type>` in `CodegenContext` carries module-level const
types and is re-seeded into `type_env` after each `type_env.clear()` in
`visit_function_for_types` and `visit_method_for_types`, so the type-inference pass can resolve
const identifiers inside function bodies.

## Recent Updates
- 2026-05-18: Added exhaustive `BinaryOp::NullCoalesce` arms in `codegen_binary` and `fold_const` (Int path); both return `CodegenError::InternalError`. Semantic-analysis gates this operator (Phase 2 feature), so reaching codegen indicates a pipeline bug — surfaced as an ICE rather than a panic so the float-fallthrough arm stays well-behaved.
- 2026-04-04: Updated `codegen_for_range` to accept `inclusive: bool` from `Stmt::ForRange` and generate `<=` (`ULE`/`SLE`) instead of `<` (`ULT`/`SLT`) comparison instructions when true.
- 2026-04-16: Implemented §1.3 const declarations end-to-end: `codegen_global_const`,
  `codegen_const_expr`, `FoldedConst` folder, `const_values`/`global_const_types` maps,
  `Stmt::Const` codegen, and type-pass support.
- 2026-04-18: Implemented bitwise operator codegen §1.4. `BinaryOp::BitAnd/BitOr/BitXor/Shl` lower
  to `build_and`/`build_or`/`build_xor`/`build_left_shift`. `UnaryOp::BitNot` lowers to `build_not`.
  `type_pass.rs` maps bitwise binary ops to the left-operand type (same as arithmetic). `fold_const`
  handles all four binary bitwise ops on `FoldedConst::Int` and `BitNot` on `FoldedConst::Int`.
- 2026-05-25: Float literal type suffixes §1.2/§1.4. `codegen_literal` dispatches `Literal::Float(val, Some(F32))` → `f32_type().const_float(val)` and `None | Some(F64)` → `f64_type().const_float(val)`. `type_pass.rs` records the matching `Type::F32` / `Type::F64` for downstream type lookups. `FoldedConst::from_literal` discards the suffix because folded float arithmetic carries the type via the consuming `Type` context.
