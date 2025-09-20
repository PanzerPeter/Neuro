---
title: NEURO Language Specification Index
---

This section documents the NEURO language features implemented in Phase 1. Each topic provides detailed information about syntax, semantics, and current implementation status with working examples.

## Quick Start
- See `overview.md` for a comprehensive language overview and design philosophy

## Core Language Features

### Basic Syntax
- `comments.md` - Comment syntax and conventions
- `literals.md` - Integer, float, string, and boolean literals
- `types.md` - Type system including primitives and tensors
- `variables.md` - Variable declarations and scoping
- `expressions.md` - All expression types and operator precedence
- `functions.md` - Function declarations, calls, and type signatures
- `semicolons.md` - Semicolon usage and statement termination

### Control Flow
- `control_if.md` - Conditional statements and branching
- `control_while.md` - While loops and flow control

### Advanced Features
- `structs.md` - Struct declarations and field access
- `modules_import.md` - Module system and import statements

## Implementation Status

### Fully Implemented ✅
- Lexical analysis (tokenization)
- Core expression evaluation
- Function declarations and calls
- Basic control flow (if/else, while loops)
- Type inference for primitives
- LLVM IR generation

### Partially Implemented ⚠️
- Struct semantics (parsing complete, limited type integration)
- Module resolution (parsing complete, full resolution in progress)
- Tensor operations (type system ready, runtime operations pending)

### Planned Features ⏳
The following features are designed but not yet implemented:
- Attributes: `#[grad]`, `#[kernel]`, `#[gpu]` for ML-specific optimizations
- For loops: `for item in collection` iteration syntax
- Enums: Algebraic data types and pattern matching
- Member access: `object.field` expressions (parsed but not semantically analyzed)
- Array indexing: `array[index]` expressions (parsed but not semantically analyzed)
- Generic functions and types
- Method call syntax
- Closures and lambda expressions

## Documentation Philosophy

Each documentation file in this specification:
- Reflects the **actual current implementation**, not planned features
- Includes working code examples that compile with the current `neurc`
- Clearly states current limitations and missing features
- Provides accurate information about what works today

This approach ensures developers have reliable information about what they can use in NEURO programs right now.

