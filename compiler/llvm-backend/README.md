# Slice: LLVM Backend

## Business Intent
Generate executable machine code from validated AST using LLVM infrastructure.

## Public Interface
- **Trigger:** `compile(items: &[Item])` function called by neurc
- **Input:** Type-checked AST items
- **Output:** `Result<Vec<u8>, CodegenError>` - Object code or compilation error
- **Reads:** Type-checked AST from neurc orchestration
- **Writes:** Object code (in-memory buffer)

## Data Ownership
- **Owns:** LLVM IR generation, code emission, optimization pipeline
- **Subscribes to:** None

## Implementation Details
Uses inkwell (LLVM bindings) to generate optimized machine code:

**LLVM Configuration:**
- LLVM version: 18.1.8 (installed at C:\LLVM-1818)
- inkwell version: 0.6.0 with llvm18-1 feature
- All targets enabled via target-all feature

**Code Generation Pipeline:**
1. Initialize LLVM context, module, builder
2. Generate function declarations
3. Generate function bodies (expressions, statements)
4. Run optimization passes (Phase 1: basic opts)
5. Emit object code

**Supported Features (Phase 1):**
- Integer arithmetic (i8, i16, i32, i64, u8, u16, u32, u64)
- Floating point (f32, f64)
- Boolean operations
- Function definitions and calls
- Variable bindings (stack allocation)
- If/else and while-loop control flow
- Loop control statements: `break`, `continue`
- Return statements
- Basic type conversions

**LLVM IR Patterns:**
- Functions use fastcc calling convention
- Variables stored on stack via alloca
- SSA form maintained via load/store
- Basic blocks for control flow

## Dependencies
- **ast-types**: AST node definitions for code generation
- **shared-types**: Type system integration
- **semantic-analysis**: No direct dependency (type checking is orchestrated by neurc)
- **diagnostics**: Compilation error reporting
- **inkwell**: LLVM bindings (0.6.0 with LLVM 18.1.8)
