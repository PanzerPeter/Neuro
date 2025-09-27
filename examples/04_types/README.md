# 04_types - Type System Features

This directory contains examples demonstrating NEURO's type system, including primitive types, type inference, and type annotations.

## Examples

### 1. [01_primitive_types.nr](./01_primitive_types.nr)
- All primitive types: `int`, `float`, `bool`, `string`
- Explicit type annotations vs type inference
- Type compatibility in operations
- Basic type usage patterns

## Key Concepts Covered

### Primitive Types
- **int**: 32-bit signed integers
- **float**: 64-bit IEEE 754 double-precision
- **bool**: Boolean values (true/false)
- **string**: UTF-8 encoded strings

### Type Annotations
- Explicit: `let x: int = 42;`
- Inferred: `let x = 42;` (compiler infers `int`)
- Function parameters: `fn add(a: int, b: int) -> int`
- Return types: Function signature type checking

### Type Safety
- Strong static typing with compile-time checking
- Type inference reduces explicit annotations
- Compatible operations within type system
- Clear error messages for type mismatches

## Running Examples

```bash
neurc run examples/04_types/01_primitive_types.nr
```

## Implementation Status

✅ **Fully Implemented**:
- All primitive types with literals
- Type inference for basic operations
- Function parameter and return type checking
- Type annotations in variable declarations

🚧 **Future Features**:
- Custom types (beyond structs)
- Generic types and type parameters
- Type aliases and advanced type constructs