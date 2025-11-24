# Operators

Operators perform operations on values (operands).

## Arithmetic Operators

### Addition (`+`)

```neuro
val sum: i32 = 10 + 20      // 30
val total: f64 = 3.14 + 2.86  // 6.00
```

**Types**: Works with numeric types (integers and floats)
**Requirement**: Both operands must be the same type

### Subtraction (`-`)

```neuro
val diff: i32 = 50 - 20     // 30
val delta: f64 = 10.5 - 2.3  // 8.2
```

**Types**: Works with numeric types
**Requirement**: Both operands must be the same type

### Multiplication (`*`)

```neuro
val product: i32 = 6 * 7    // 42
val area: f64 = 3.14 * 2.0  // 6.28
```

**Types**: Works with numeric types
**Requirement**: Both operands must be the same type

### Division (`/`)

```neuro
val quotient: i32 = 20 / 4  // 5
val ratio: f64 = 10.0 / 3.0  // 3.333...
```

**Types**: Works with numeric types
**Requirement**: Both operands must be the same type
**Note**: Integer division truncates (5 / 2 = 2)

### Modulo (`%`)

```neuro
val remainder: i32 = 17 % 5  // 2
val mod: i32 = 10 % 3        // 1
```

**Types**: Works with integer types
**Requirement**: Both operands must be integers

## Comparison Operators

All comparison operators return `bool`.

### Equal (`==`)

```neuro
val is_equal: bool = 42 == 42  // true
val same: bool = x == y
```

### Not Equal (`!=`)

```neuro
val is_different: bool = 42 != 10  // true
val not_same: bool = x != y
```

### Less Than (`<`)

```neuro
val is_less: bool = 5 < 10  // true
val smaller: bool = x < y
```

### Greater Than (`>`)

```neuro
val is_greater: bool = 10 > 5  // true
val larger: bool = x > y
```

### Less Than or Equal (`<=`)

```neuro
val is_lte: bool = 5 <= 5  // true
val at_most: bool = x <= max
```

### Greater Than or Equal (`>=`)

```neuro
val is_gte: bool = 10 >= 5  // true
val at_least: bool = x >= min
```

**Types**: Work with numeric types and booleans
**Requirement**: Both operands must be the same type

## Logical Operators

Work with boolean values, return boolean.

### Logical AND (`&&`)

```neuro
val both: bool = true && true    // true
val result: bool = flag1 && flag2
val valid: bool = x > 0 && x < 100
```

**Short-circuit**: If left side is `false`, right side is not evaluated

### Logical OR (`||`)

```neuro
val either: bool = true || false  // true
val result: bool = flag1 || flag2
val valid: bool = x < 0 || x > 100
```

**Short-circuit**: If left side is `true`, right side is not evaluated

### Logical NOT (`!`)

```neuro
val inverted: bool = !true  // false
val opposite: bool = !flag
```

**Unary operator**: Takes single boolean operand

## Unary Operators

### Negation (`-`)

```neuro
val neg: i32 = -42       // -42
val opposite: i32 = -x
val abs_neg: i32 = -abs(x)
```

**Types**: Works with numeric types
**Returns**: Same type as operand

### Logical NOT (`!`)

```neuro
val not_true: bool = !true  // false
val not_flag: bool = !flag
```

**Types**: Works with boolean type only
**Returns**: boolean

## Assignment Operator (`=`)

### Variable Assignment

```neuro
mut x: i32 = 10
x = 20              // Reassign
x = x + 5           // Update
x = add(x, 10)      // Assign from expression
```

**Requirement**: Variable must be declared with `mut`
**Type checking**: Right-hand side must match variable type

### Cannot Assign to Immutable

```neuro
val x: i32 = 10
// x = 20  // Error: cannot assign to immutable variable
```

## Operator Precedence

From highest to lowest:

| Precedence | Operators | Example |
|------------|-----------|---------|
| 7 | `-` (unary), `!` | `-x`, `!flag` |
| 6 | `*`, `/`, `%` | `a * b`, `x / y`, `n % 2` |
| 5 | `+`, `-` | `a + b`, `x - y` |
| 4 | `<`, `>`, `<=`, `>=` | `x < y`, `a >= b` |
| 3 | `==`, `!=` | `x == y`, `a != b` |
| 2 | `&&` | `a && b` |
| 1 | `\|\|` | `a \|\| b` |

### Precedence Examples

```neuro
a + b * c       // Same as: a + (b * c)
a * b + c       // Same as: (a * b) + c
a < b == c < d  // Same as: (a < b) == (c < d)
!a && b         // Same as: (!a) && b
a || b && c     // Same as: a || (b && c)
```

### Using Parentheses

```neuro
(a + b) * c     // Force addition first
a * (b + c)     // Force addition before multiplication
(a && b) || c   // Force AND before OR (though same as default)
```

## Type Requirements

### Numeric Operators

`+`, `-`, `*`, `/` work with:
- `i8`, `i16`, `i32`, `i64`
- `u8`, `u16`, `u32`, `u64`
- `f32`, `f64`

Both operands must be the same type.

### Integer-Only Operators

`%` works only with integer types:
- `i8`, `i16`, `i32`, `i64`
- `u8`, `u16`, `u32`, `u64`

### Comparison Operators

`==`, `!=`, `<`, `>`, `<=`, `>=` work with:
- All numeric types (same type required)
- `bool` (only `==` and `!=`)

### Logical Operators

`&&`, `||`, `!` work only with `bool`

## Common Patterns

### Range Checking

```neuro
val in_range: bool = x >= min && x <= max
val out_of_range: bool = x < min || x > max
```

### Clamping

```neuro
val clamped: i32 = if x < min {
    min
} else if x > max {
    max
} else {
    x
}
```

### Sign Determination

```neuro
val sign: i32 = if x > 0 {
    1
} else if x < 0 {
    -1
} else {
    0
}
```

### Absolute Value

```neuro
val abs: i32 = if x >= 0 { x } else { -x }
```

## Operator Overloading

Operator overloading is not supported in Phase 1.

Future phases may support custom operator implementations for user-defined types.

## Common Mistakes

### Type Mismatch

```neuro
val x: i32 = 10
val y: f64 = 3.14
// val z = x + y  // Error: cannot add i32 and f64
```

### Integer Division

```neuro
val result: i32 = 5 / 2  // Result is 2, not 2.5
```

Use floats for decimal division:

```neuro
val result: f64 = 5.0 / 2.0  // Result is 2.5
```

### Boolean Comparison

```neuro
val flag: bool = true
// if flag == true { }  // Redundant
if flag { }             // Better
```

## References

- [Types](types.md) - Type requirements for operators
- [Expressions](expressions.md) - Operator precedence and evaluation
- [Variables](variables.md) - Assignment operator
