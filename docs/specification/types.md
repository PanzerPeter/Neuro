# Types

NEURO has a static type system with the following implemented types in Phase 1:

## Primitive Types

### Integer Types
- `int`: 32-bit signed integer (-2^31 to 2^31-1)

### Floating-Point Types
- `float`: 64-bit IEEE 754 double-precision floating-point

### Boolean Type
- `bool`: Boolean type with values `true` and `false`

### String Type
- `string`: UTF-8 encoded string literals with escape sequence support

## Tensor Types

NEURO provides first-class tensor support for machine learning workloads:

- `Tensor<T, [dims]>`: Multi-dimensional array with element type `T` and compile-time known dimensions
  - `T` can be `int`, `float`, `bool`
  - `dims` is a compile-time constant array specifying dimensions
  - `?` is permitted in the parser as a placeholder for dynamic dimensions (not yet implemented)

Examples:
- `Tensor<float, [5]>`: Vector of 5 floats
- `Tensor<int, [2, 3]>`: 2x3 matrix of integers
- `Tensor<bool, [10, 10, 3]>`: 3D tensor for boolean data

## Function Types

Function types are denoted as `fn(param_types...) -> return_type`:
- `fn() -> int`: Function taking no parameters and returning an integer
- `fn(int, float) -> bool`: Function taking an int and float, returning bool
- `fn()`: Function returning void (no explicit return type)

## Type Inference

The NEURO compiler includes type inference capabilities:
- `?` is used internally for unknown/inferred types
- Generic identifiers are parsed but not yet resolved generically
- Type annotations can often be omitted due to inference

## Examples

```neuro
// Function with tensor return type
fn make() -> Tensor<float, [2, 3]> {
    return 0; // placeholder implementation
}

// Simple function with type inference
fn id(x: int) -> int {
    return x;
}

// Variable declarations with explicit types
let a: int = 42;
let b: float = 3.14;
let s: string = "hello";
let flag: bool = true;
```

## Current Limitations

- Generic type parameters are parsed but not yet fully implemented
- Dynamic tensor dimensions (using `?`) are parsed but not yet supported in the type checker
- User-defined types (structs, enums) have limited integration with the type system

