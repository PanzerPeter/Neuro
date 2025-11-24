# Type System

NEURO is a statically typed language with explicit type annotations and planned type inference.

## Current Status (Phase 1, ~92% Complete)

- ✅ Primitive types (integers, floats, booleans)
- ✅ Extended integer types (i8-i64, u8-u64)
- ✅ Function types
- ✅ Void type
- ⏳ Type inference for numeric literals (pending)
- ⏳ String type (pending)
- ⏸️ Structs (Phase 2)
- ⏸️ Arrays (Phase 2)
- ⏸️ Tuples (Phase 2)
- ⏸️ Generics (Phase 2)
- ⏸️ Traits (Phase 2)

## Primitive Types

### Integer Types

NEURO supports 8 integer types with different sizes and signedness:

#### Signed Integers

| Type | Size | Range |
|------|------|-------|
| `i8` | 8-bit | -128 to 127 |
| `i16` | 16-bit | -32,768 to 32,767 |
| `i32` | 32-bit | -2,147,483,648 to 2,147,483,647 |
| `i64` | 64-bit | -9,223,372,036,854,775,808 to 9,223,372,036,854,775,807 |

#### Unsigned Integers

| Type | Size | Range |
|------|------|-------|
| `u8` | 8-bit | 0 to 255 |
| `u16` | 16-bit | 0 to 65,535 |
| `u32` | 32-bit | 0 to 4,294,967,295 |
| `u64` | 64-bit | 0 to 18,446,744,073,709,551,615 |

**Examples**:

```neuro
func demo_integers() -> i32 {
    val tiny: i8 = 127           // Smallest signed
    val small: i16 = 32767       // Medium signed
    val normal: i32 = 2147483647 // Default signed
    val big: i64 = 9223372036854775807  // Largest signed

    val byte: u8 = 255           // Smallest unsigned
    val word: u16 = 65535        // Medium unsigned
    val dword: u32 = 4294967295  // Large unsigned
    val qword: u64 = 18446744073709551615  // Largest unsigned

    return normal
}
```

**Default Type**: Currently, integer literals without explicit type annotation default to `i32`. Type inference is pending in Phase 1.

### Floating-Point Types

| Type | Size | Precision | Range (approx) |
|------|------|-----------|----------------|
| `f32` | 32-bit | ~7 decimal digits | ±1.18e-38 to ±3.40e38 |
| `f64` | 64-bit | ~15 decimal digits | ±2.23e-308 to ±1.80e308 |

**Examples**:

```neuro
func demo_floats() -> f64 {
    val pi: f32 = 3.14159        // Single precision
    val e: f64 = 2.71828182845   // Double precision (default)
    val sci: f64 = 1.23e10       // Scientific notation

    return e
}
```

**Default Type**: Float literals default to `f64`. Type inference is pending in Phase 1.

### Boolean Type

The `bool` type represents truth values:

```neuro
func demo_booleans() -> i32 {
    val is_true: bool = true
    val is_false: bool = false

    if is_true {
        return 1
    } else {
        return 0
    }
}
```

**Values**: `true` or `false`
**Operations**: Logical operators (`&&`, `||`, `!`), comparison results

## Void Type

Functions that don't return a value have implicit `void` return type:

```neuro
func print_debug() {
    // No return type specified = void
    // Implicit return at end of function
}

// Explicit void (optional, rarely used)
func print_debug_explicit() -> void {
    return
}
```

**Note**: The `main` function must return `i32` (exit code), not void.

## Type Annotations

### Variable Declarations

Type annotations are required for variable declarations in Phase 1:

```neuro
val x: i32 = 42              // Explicit type annotation
val pi: f64 = 3.14159        // Explicit type annotation
val flag: bool = true        // Explicit type annotation
```

**Planned (Phase 1 completion)**:
```neuro
val x = 42              // Will infer i32
val pi = 3.14159        // Will infer f64
val flag = true         // Will infer bool
```

### Function Parameters

Function parameters must have explicit type annotations:

```neuro
func add(a: i32, b: i32) -> i32 {
    return a + b
}
```

### Function Return Types

Return types must be explicitly specified (or omitted for void):

```neuro
func returns_int() -> i32 {
    return 42
}

func returns_float() -> f64 {
    return 3.14
}

func returns_nothing() {
    // Implicit void return
}
```

## Type Compatibility

### Strict Type System

NEURO uses strict type checking with no implicit conversions in Phase 1:

```neuro
func strict_types() -> i32 {
    val x: i32 = 42
    val y: i64 = x  // Error: type mismatch (i32 vs i64)
    return y
}
```

Even compatible types require explicit conversion (Phase 2 feature).

### Function Type Checking

Function calls are type-checked strictly:

```neuro
func takes_i32(x: i32) -> i32 {
    return x
}

func test() -> i32 {
    val x: i64 = 100
    return takes_i32(x)  // Error: expected i32, found i64
}
```

### Return Type Checking

All return statements must match the declared return type:

```neuro
func returns_i32() -> i32 {
    if true {
        return 42       // OK: i32
    } else {
        return 3.14     // Error: expected i32, found f64
    }
}
```

## Type System Features

### Phase 1 (Current, ~92%)

**Implemented**:
- Primitive types (i8-i64, u8-u64, f32, f64, bool)
- Explicit type annotations
- Function types
- Strict type checking
- Type mismatch error reporting

**Pending**:
- Type inference for numeric literals
- Basic string type

### Phase 2 (Planned)

- Type inference for variable declarations
- Structs (user-defined types)
- Arrays with fixed size
- Tuples for grouping values
- Generic functions with monomorphization
- Explicit type conversions (`as` operator)

### Phase 3 (Planned)

- Static tensor types: `Tensor<f32, [3, 3]>`
- Compile-time shape checking
- Broadcasting rules

### Phase 4+ (Future)

- Traits (type classes)
- Associated types
- Dynamic tensor shapes
- Advanced type system features

## Type Safety Guarantees

NEURO's type system provides:

1. **No undefined behavior from type errors**: All type errors caught at compile time
2. **No implicit conversions**: Explicit is better than implicit
3. **Function type safety**: Arguments and returns type-checked
4. **Memory safety**: Types prevent invalid memory access (future: ownership system)

## Common Type Errors

### Type Mismatch

```neuro
func mismatch() -> i32 {
    val x: i32 = true  // Error: expected i32, found bool
    return x
}
```

**Error message**:
```
Type error: Type mismatch
  expected: i32
  found: bool
  at program.nr:2:18
```

### Argument Type Mismatch

```neuro
func takes_i32(x: i32) -> i32 {
    return x
}

func wrong_arg() -> i32 {
    return takes_i32(true)  // Error: expected i32, found bool
}
```

**Error message**:
```
Type error: Argument type mismatch
  expected: i32
  found: bool
  at program.nr:6:22
```

### Return Type Mismatch

```neuro
func returns_wrong() -> i32 {
    return true  // Error: expected i32, found bool
}
```

**Error message**:
```
Type error: Return type mismatch
  expected: i32
  found: bool
  at program.nr:2:12
```

## Best Practices

### 1. Choose Appropriate Integer Types

```neuro
// Good: use smallest type that fits
val age: u8 = 25          // Ages fit in u8 (0-255)
val year: u16 = 2025      // Years fit in u16
val file_size: u64 = 1000000000  // Large files need u64

// Avoid: unnecessarily large types
val counter: i64 = 0      // Wasteful if i32 suffices
```

### 2. Use f64 for Most Floating-Point Math

```neuro
// Good: f64 for precision
val pi: f64 = 3.141592653589793

// Only use f32 when:
// - Memory is constrained
// - Precision is not critical
// - Interfacing with f32 APIs
```

### 3. Be Explicit About Types

Even with future type inference, explicit types improve readability:

```neuro
// Clear intent
func calculate_area(radius: f64) -> f64 {
    val pi: f64 = 3.14159
    return pi * radius * radius
}
```

### 4. Use Booleans for Flags

```neuro
// Good: boolean for true/false
val is_valid: bool = true

// Avoid: integer for boolean logic
val is_valid: i32 = 1  // Less clear
```

## Type Conversion (Phase 2)

Explicit type conversions will be available in Phase 2:

```neuro
// Future feature (not yet implemented)
func convert_types() -> i64 {
    val x: i32 = 42
    val y: i64 = x as i64  // Explicit conversion
    return y
}
```

Until then, all types must match exactly.

## Examples

### Working with Multiple Types

```neuro
func compute(a: i32, b: f64) -> f64 {
    // Error in Phase 1: cannot mix i32 and f64
    // return a + b  // Type mismatch

    // Future (Phase 2): explicit conversion
    // return (a as f64) + b

    // Phase 1 workaround: use same types
    return b * 2.0
}
```

### Type-Safe Function Composition

```neuro
func double(x: i32) -> i32 {
    x * 2
}

func add_ten(x: i32) -> i32 {
    x + 10
}

func compose() -> i32 {
    val x: i32 = 5
    val y: i32 = double(x)      // 10
    val z: i32 = add_ten(y)     // 20
    z
}
```

## References

- [Variables](variables.md) - Variable declaration and usage
- [Functions](functions.md) - Function types and signatures
- [Operators](operators.md) - Type requirements for operators
- [Expressions](expressions.md) - Expression type checking

## See Also

- Rust Book: [Data Types](https://doc.rust-lang.org/book/ch03-02-data-types.html)
- [Type System Design](https://en.wikipedia.org/wiki/Type_system)
