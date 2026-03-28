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

## Future: MLIR Integration (Phase 3+)
When tensor operations are introduced, `melior` (Rust MLIR bindings, targeting the same
LLVM 20 / MLIR 20 installation) will be added alongside inkwell. The lowering strategy
will be: AST → NEURO High-Level IR → MLIR dialects (linalg/tensor/func/arith) →
Enzyme MLIR AD pass → GPU dialects (nvgpu/rocdl) or `llvm` dialect → inkwell for final
LLVM IR emission. inkwell remains the terminal code-emission layer in all paths.
