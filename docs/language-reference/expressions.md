# Expressions

Expressions are code constructs that evaluate to a value.

## Expression Types

### Literals

Values written directly in code:

```neuro
42              // Integer literal (i32)
3.14            // Float literal (f64)
true            // Boolean literal
false           // Boolean literal
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
-x              // Negation
!flag           // Logical NOT
```

### Function Call Expressions

```neuro
add(5, 3)                   // Simple call
double(square(x))           // Nested calls
max(min(a, b), c)           // Multiple nesting
```

### Parenthesized Expressions

Control evaluation order:

```neuro
(a + b) * c     // Force addition first
x / (y + z)     // Force addition before division
```

## Operator Precedence

Higher precedence operators evaluate first:

| Precedence | Operators | Associativity | Description |
|------------|-----------|---------------|-------------|
| 7 (Highest) | `-` (unary), `!` | Right | Unary negation, logical NOT |
| 6 | `*`, `/`, `%` | Left | Multiplication, division, modulo |
| 5 | `+`, `-` | Left | Addition, subtraction |
| 4 | `<`, `>`, `<=`, `>=` | Left | Comparisons |
| 3 | `==`, `!=` | Left | Equality |
| 2 | `&&` | Left | Logical AND |
| 1 (Lowest) | `\|\|` | Left | Logical OR |

**Examples**:

```neuro
a + b * c       // Parsed as: a + (b * c)
a < b == c < d  // Parsed as: (a < b) == (c < d)
!a && b         // Parsed as: (!a) && b
a || b && c     // Parsed as: a || (b && c)
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
