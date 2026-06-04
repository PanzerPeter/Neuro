# Expressions

Expressions are code constructs that evaluate to a value.

## Expression Types

### Literals

Values written directly in code:

```neuro
42              // Integer literal (i32 by default)
3.14            // Float literal (f64 by default)
true            // Boolean literal
false           // Boolean literal
"hello"         // String literal (type: string)
```

### Identifiers

Variable and function names:

```neuro
x               // Variable reference
add             // Function reference
```

### Binary Expressions

Two operands with an operator:

```neuro
a + b           // Addition
x * y           // Multiplication
result == 42    // Comparison
flag && ready   // Logical AND
```

### Unary Expressions

Single operand with operator:

```neuro
-x              // Arithmetic negation
!flag           // Logical NOT
~mask           // Bitwise NOT (integer types only)
```

### Cast Expressions

Explicit numeric type conversion:

```neuro
n as f64        // Integer to float
pi as i32       // Float to integer (truncates toward zero)
flag as i32     // Boolean to integer (false → 0, true → 1)
```

### Function Call Expressions

```neuro
add(5, 3)                   // Free function call
double(square(x))           // Nested calls
max(min(a, b), c)           // Multiple nesting
c.add(32)                   // Method call (instance method)
Point::new(1, 2)            // Associated function call
"hello".len()               // Builtin method on a string receiver
```

Method-call syntax `receiver.method(args)` resolves against user-defined `impl` methods
when the receiver is a struct, and against a fixed, compiler-known set of intrinsic methods
when the receiver is a builtin (primitive or string) type. The first builtin intrinsic is
`string.len()` — see [types.md](types.md#string-methods).

### Struct Literal Expressions

Construct a struct value by naming all fields:

```neuro
val p = Point { x: 3.0, y: 4.0 }
val c = Counter { value: 0, step: 1 }
```

All fields must be present; duplicate or unknown fields are compile errors.

### Field Access Expressions

Read a field from a struct value:

```neuro
val x = p.x          // reads field x — type is f64
val total = c.value + c.step
```

Field access binds tighter than function calls in the precedence table.

### If Expressions

`if`/`else` chains are expressions when every arm has an `else` branch:

```neuro
val abs_n: i32 = if n >= 0 { n } else { 0 - n }
val sign: i32  = if n < 0 { -1 } else if n == 0 { 0 } else { 1 }
```

All arms must produce the same type. An `if` without `else` has type `Void` and cannot be used as a value.

### Block Expressions

A `{ … }` block is an expression whose value is its final (trailing) expression:

```neuro
val area: i32 = {
    val w: i32 = 6
    val h: i32 = 7
    w * h           // trailing expression — this is the block's value
}
```

Locals declared inside a block are scoped to that block.

### Unsafe Block Expressions

An `unsafe { … }` block is a block expression prefixed with the reserved
`unsafe` keyword. It evaluates exactly like a bare block — its value is the
trailing expression, and its locals are block-scoped:

```neuro
val x: i32 = unsafe {
    val a: i32 = 20
    a + 22          // trailing expression — this is the block's value
}
```

`unsafe` is currently **inert**: it is a reserved keyword and produces a
distinct AST node, but carries no special semantics yet. It exists as Phase 1.7
groundwork for the GPU-kernel aliasing model (Phase 5), where `unsafe { }` will
gate raw `KernelOut` index writes. Until then it behaves identically to `{ }`.

### Parenthesized Expressions

Control evaluation order:

```neuro
(a + b) * c     // Force addition first
x / (y + z)     // Force addition before division
```

## Operator Precedence

Higher precedence operators evaluate first. Full table from highest to lowest:

| Level | Operators | Associativity | Description |
|-------|-----------|---------------|-------------|
| (highest) | `.` | Left | Field access |
| | `(…)` | Left | Function call / method call |
| 2 | `-` (unary), `!`, `~` | Right | Negation, logical NOT, bitwise NOT |
| 3 | `as` | Left | Type cast |
| 5 | `*`, `/`, `%` | Left | Multiply, divide, modulo |
| 6 | `+`, `-` | Left | Addition, subtraction |
| 7 | `<<` | Left | Left shift |
| 8 | `&` | Left | Bitwise AND |
| 9 | `^` | Left | Bitwise XOR |
| 10 | `\|` | Left | Bitwise OR |
| 11 | `<`, `>`, `<=`, `>=`, `==`, `!=` | None | Comparison / equality |
| 12 | `&&` | Left | Logical AND |
| 13 (lowest) | `\|\|` | Left | Logical OR |

**Examples**:

```neuro
a + b * c         // Parsed as: a + (b * c)
a & b + c         // Parsed as: a & (b + c)   — arithmetic before bitwise
a | b & c         // Parsed as: a | (b & c)   — AND before OR
a < b == c < d    // Parsed as: (a < b) == (c < d)
!a && b           // Parsed as: (!a) && b
a || b && c       // Parsed as: a || (b && c)
n as f64 + 1.0    // Parsed as: (n as f64) + 1.0
```

## Expression-Based Returns

Last expression in a function (without semicolon) is the return value:

```neuro
func add(a: i32, b: i32) -> i32 {
    a + b  // Implicit return
}

func max(a: i32, b: i32) -> i32 {
    if a > b {
        a  // Implicit return from if branch
    } else {
        b  // Implicit return from else branch
    }
}
```

**Key distinction**:
- Expression (no semicolon): Returns value
- Statement (with semicolon): Evaluates but doesn't return

```neuro
func example() -> i32 {
    val x: i32 = 42  // Statement (semicolon required)
    x  // Expression (no semicolon, implicit return)
}
```

## Type Checking

All expressions are type-checked at compile time:

```neuro
val x: i32 = 42 + 10        // OK: i32 + i32 = i32
val y: f64 = 3.14 * 2.0     // OK: f64 * f64 = f64
val z: bool = x > y         // Error: cannot compare i32 and f64
```

## Evaluation Order

Left-to-right evaluation for operators of same precedence:

```neuro
a + b + c       // Evaluates as: (a + b) + c
a - b - c       // Evaluates as: (a - b) - c
a * b / c       // Evaluates as: (a * b) / c
```

Function arguments are evaluated left-to-right:

```neuro
add(first(), second())  // first() called before second()
```

## Expression Examples

### Arithmetic Expressions

```neuro
val sum: i32 = a + b
val product: i32 = x * y * z
val average: i32 = (a + b + c) / 3
val remainder: i32 = n % 10
```

### Comparison Expressions

```neuro
val is_equal: bool = x == y
val is_greater: bool = a > b
val in_range: bool = x >= 0 && x <= 100
```

### Logical Expressions

```neuro
val both_true: bool = flag1 && flag2
val either_true: bool = flag1 || flag2
val inverted: bool = !flag
val complex: bool = (a && b) || (c && d)
```

### Nested Function Calls

```neuro
val result: i32 = add(mul(2, 3), div(10, 2))
val distance: i32 = abs(x1 - x2) + abs(y1 - y2)
```

## Common Patterns

### Chained Comparisons

```neuro
val in_range: bool = min <= value && value <= max
val outside_range: bool = value < min || value > max
```

### Conditional Values

```neuro
val sign: i32 = if x > 0 { 1 } else if x < 0 { -1 } else { 0 }
val abs_value: i32 = if x >= 0 { x } else { -x }
```

### Expression Composition

```neuro
val result: i32 = square(x) + square(y)
val total: i32 = sum(a, b) + sum(c, d)
```

## References

- [Operators](operators.md) - Detailed operator documentation
- [Types](types.md) - Expression type checking
- [Functions](functions.md) - Function call expressions
