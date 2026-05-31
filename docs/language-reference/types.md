# Type System

Neuro is a statically typed language with explicit type annotations and planned type inference.

## Current Status

- Implemented: primitive types (integers, floats, booleans)
- Implemented: extended integer types (`i8`-`i64`, `u8`-`u64`)
- Implemented: function types
- Implemented: void type
- Implemented: contextual inference for numeric literals
- Implemented: string type
- Implemented: structs (definition, instantiation, field access, field mutation)
- Planned (Phase 2): arrays
- Planned (Phase 2): tuples
- Planned (Phase 2): generics
- Planned (Phase 2): traits

## Primitive Types

### Integer Types

Neuro supports 8 integer types with different sizes and signedness:

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

**Default Type**: Integer literals default to `i32` when no annotation is present. Contextual inference from declaration, parameter, and return context is implemented; range validation is enforced (e.g. `300` cannot be assigned to `i8`). If an unannotated integer literal exceeds the range of `i32` (e.g. `5000000000`), a compile error is emitted. It is not silently promoted to `i64`.

**Type Suffixes**: A suffix appended directly to an integer literal overrides contextual inference and pins the type:

```neuro
val a = 42i64      // i64, no annotation needed
val b = 255u8      // u8
val c = 0xFFu8     // hex literal with suffix
val d = 0b1010i32  // binary literal with suffix
```

Valid suffixes: `i8`, `i16`, `i32`, `i64`, `u8`, `u16`, `u32`, `u64`. The value is range-checked against the suffix type at compile time — `300u8` is a compile error.

#### Integer Overflow

When a runtime `+`, `-`, or `*` produces a result outside the range of its integer type, the behavior depends on the optimization level the program was compiled with:

| Build | Flag | Overflow behavior |
|-------|------|-------------------|
| Debug | `-O0` (default) | The program **aborts** at runtime (traps). |
| Release | `-O1`, `-O2`, `-O3` | The result **wraps** using two's complement. |

```neuro
func main() -> i32 {
    mut x: u8 = 200u8
    val y: u8 = 100u8
    val z: u8 = x + y   // 300 > u8::MAX
    // Debug (-O0):   aborts here
    // Release (-O2): wraps to 44
    return z as i32
}
```

The debug-build trap turns a silent miscalculation into an immediate failure during development, while release builds match the zero-overhead wrapping behavior of the underlying hardware. The check is applied to `+`, `-`, and `*` only; division and modulo are unaffected. Compile-time constant folding always uses wrapping arithmetic regardless of optimization level.

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

**Default Type**: Float literals default to `f64`. Contextual inference from declaration, parameter, and return context is implemented.

**Type Suffixes**: A suffix appended directly to a float literal overrides contextual inference and pins the type:

```neuro
val a = 1.5f32        // f32, no annotation needed
val b = 2.0f64        // f64
val c = 1e10f32       // exponent form with suffix
val d = 1.5e-5f64     // fractional + exponent with suffix
```

Valid suffixes: `f32`, `f64`. The suffix attaches directly to the literal — no whitespace is permitted between the digits and the suffix. The exponent form (`1e10f32`) and the fractional form (`1.5f32`) both accept a suffix.

### Digit Separators

Underscores may be placed between digits of any numeric literal to improve readability. They carry no value — the compiler strips them before parsing — and work in every base, in floats, in exponents, and alongside type suffixes.

```neuro
val million = 1_000_000      // decimal grouping
val mask    = 0xFF_FF        // hex
val flags   = 0b1010_0011    // binary
val perms   = 0o7_5_5        // octal
val ratio   = 1_000.000_5    // float
val scaled  = 1_0e1_0        // exponent
val wide    = 1_000_000i64   // with a type suffix
```

A separator is only recognized between digits: a leading underscore (`_1000`) is an identifier, not a number.

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

## String Type

The `string` type is an immutable, UTF-8 encoded fat pointer `{ ptr, i64 }` — a pointer to
the bytes plus a stored byte length. Equality (`==`, `!=`) compares byte content.

### String Methods

Builtin intrinsic methods dispatch on a `string` receiver via the usual `receiver.method()`
syntax:

```neuro
val s: string = "hello, world"
val n: u64 = s.len()    // 12 — O(1) read of the stored byte length
```

**`.len() -> u64`** — returns the number of UTF-8 bytes, read directly from the fat pointer
in O(1) with no scan. The length **excludes** the null terminator. Because the index is a
byte count, a multi-byte code point contributes more than one to the length.

## Struct Types

Structs are user-defined types that group named fields. They use nominal typing — two structs with identical fields are distinct types.

### Definition

```neuro
struct Point {
    x: f64,
    y: f64
}

struct Counter {
    value: i32,
    step: i32
}
```

Fields are listed as `name: Type`, separated by commas or newlines. Any primitive type (or another struct type) is valid as a field type.

### Instantiation

```neuro
val p = Point { x: 3.0, y: 4.0 }
val c = Counter { value: 0, step: 1 }
```

All fields must be provided. Extra or missing fields are compile errors.

### Field Access

```neuro
val x_coord = p.x   // reads field x from p
val total = c.value + c.step
```

Field access resolves to the declared field type.

### Field Mutation

Field mutation is only allowed on `mut` bindings:

```neuro
mut cursor = Point { x: 0.0, y: 0.0 }
cursor.x = 5.0   // OK: cursor is mut

val fixed = Point { x: 1.0, y: 2.0 }
fixed.x = 3.0    // Error: AssignToImmutableField
```

### Definition Order

Structs can be used before they are defined in the source file — the compiler performs a pre-registration pass:

```neuro
func main() -> i32 {
    val s = Score { value: 42 }
    return s.value
}

struct Score {
    value: i32
}
```

### Type Errors

| Error | Cause |
|---|---|
| `MissingStructField` | Struct literal omits a declared field |
| `UnknownField` | Struct literal or access uses a field that doesn't exist |
| `AssignToImmutableField` | Field assignment on a `val` binding |
| `StructAlreadyDefined` | Two `struct` declarations share the same name |
| `UnknownStruct` | Struct literal references an undeclared struct name |

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

Type annotations are optional when the type can be inferred from context:

```neuro
val x: i32 = 42              // Explicit type annotation
val pi: f64 = 3.14159        // Explicit type annotation
val flag: bool = true        // Explicit type annotation
val n = 100                  // Inferred i32 (default for integer literals)
val pi = 3.14159             // Inferred f64 (default for float literals)
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

Neuro uses strict type checking with no implicit conversions in Phase 1:

```neuro
func strict_types() -> i32 {
    val x: i32 = 42
    val y: i64 = x  // Error: type mismatch (i32 vs i64)
    return y
}
```

Even compatible types require explicit conversion.

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

### Phase 1 (Complete ✅)

**Implemented**:
- Primitive types (i8-i64, u8-u64, f32, f64, bool)
- String type with fat pointer ABI (`{ ptr, i64 }`)
- Explicit type annotations
- Contextual numeric literal inference with range validation
- Explicit type conversions via `as` operator
- Function types
- Strict type checking
- Type mismatch error reporting

### Phase 2 (In Progress)

- ✅ Structs (user-defined types with nominal typing)
- Type inference for variable declarations
- Arrays with fixed size
- Tuples for grouping values
- Generic functions with monomorphization

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

Neuro's type system provides:

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

## Type Conversion

Explicit type conversions are supported via the `as` operator. There are no implicit type conversions in Neuro.

```neuro
func convert_types() -> i64 {
    val x: i32 = 42
    val y: i64 = x as i64      // Explicit conversion (widening)
    val f: f64 = y as f64      // Int to float
    
    val pi: f64 = 3.14
    val trunc: i32 = pi as i32 // Float to int (truncates)
    
    val flag: bool = true
    val num: i32 = flag as i32 // Boolean to int (1)
    
    return y
}
```

The compiler will reject invalid casts (e.g. casting a string to an integer).

## Examples

### Working with Multiple Types

```neuro
func compute(a: i32, b: f64) -> f64 {
    // Mix i32 and f64 by casting one
    // return a + b  // ERROR: Type mismatch

    // Explicit conversion
    return (a as f64) + b

    
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
