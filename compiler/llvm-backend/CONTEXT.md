# llvm-backend

## Purpose
Emit native object code from a type-checked NEURO AST via LLVM IR generation.

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
inkwell 0.8.0 with feature `llvm20-1` (LLVM 20 bindings) is a third-party crate, not NEURO-owned Shared Kernel.
Requires LLVM 20 installed with MLIR enabled; set `LLVM_SYS_200_PREFIX` to the LLVM 20
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

## Future: MLIR Integration (Phase 3+)
When tensor operations are introduced, `melior` (Rust MLIR bindings, targeting the same
LLVM 20 / MLIR 20 installation) will be added alongside inkwell. The lowering strategy
will be: AST → NEURO High-Level IR → MLIR dialects (linalg/tensor/func/arith) →
Enzyme MLIR AD pass → GPU dialects (nvgpu/rocdl) or `llvm` dialect → inkwell for final
LLVM IR emission. inkwell remains the terminal code-emission layer in all paths.
