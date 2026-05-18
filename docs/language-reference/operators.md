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

**Types**: Work with numeric types, booleans, and strings (`==`/`!=` only)
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

## Bitwise Operators

Work with integer values only (`i8`–`i64`, `u8`–`u64`). Cannot be used with floats or bools.

### Bitwise AND (`&`)

```neuro
val a: i32 = 0b1100   // 12
val b: i32 = 0b1010   // 10
val r: i32 = a & b    // 0b1000 = 8
```

**Returns**: same type as operands

### Bitwise OR (`|`)

```neuro
val a: i32 = 0b1100   // 12
val b: i32 = 0b1010   // 10
val r: i32 = a | b    // 0b1110 = 14
```

**Returns**: same type as operands

### Bitwise XOR (`^`)

```neuro
val a: i32 = 0b1100   // 12
val b: i32 = 0b1010   // 10
val r: i32 = a ^ b    // 0b0110 = 6
```

**Returns**: same type as operands

### Left Shift (`<<`)

```neuro
val a: i32 = 1
val r: i32 = a << 4   // 1 * 2^4 = 16
```

**Returns**: same type as operands
**Note**: Right shift is exposed as the `.shr(n)` method (Phase 2+), not an operator.

### Bitwise NOT (`~`)

```neuro
val a: i32 = 0
val r: i32 = ~a       // -1 (all bits set, two's complement)
```

**Unary**: takes a single integer operand
**Returns**: same type as operand

## Type Casting Operator (`as`)

Performs an explicit numeric or boolean type conversion.

```neuro
val n: i32 = 42
val x: f64 = n as f64          // widen integer to float
val y: i64 = n as i64          // widen to larger integer

val pi: f64 = 3.14159
val trunc: i32 = pi as i32     // truncate toward zero → 3

val flag: bool = true
val one: i32 = flag as i32     // false → 0, true → 1
```

**Types**: Works with numeric types and booleans.
**Rules**:
- Widening integers zero-extends (unsigned) or sign-extends (signed).
- Floats to integers truncate towards zero.
- Booleans to integers map `false → 0` and `true → 1`.

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

## Compound Assignment Operators

Shorthand for updating a mutable variable in-place. Each form is equivalent to
a plain assignment with the corresponding binary operator on the right-hand side.

| Operator | Equivalent to |
|----------|---------------|
| `x += n` | `x = x + n`  |
| `x -= n` | `x = x - n`  |
| `x *= n` | `x = x * n`  |
| `x /= n` | `x = x / n`  |
| `x %= n` | `x = x % n`  |

```neuro
mut score: i32 = 100
score += 50    // 150
score -= 25    // 125
score *= 2     // 250
score /= 5     // 50
score %= 13    // 11
```

```neuro
mut sum: i32 = 0
mut i: i32 = 1
while i <= 10 {
    sum += i
    i += 1
}
```

**Requirement**: Left-hand side must be a `mut` variable
**Type checking**: Same rules as the underlying binary operator apply
**Note**: Compound assignment on struct fields (`point.x += 1.0`) is not yet supported

## Null/Error Coalescing Operator (`??`)

`??` is the read-site equivalent of `unwrap_or(default)` — it returns the unwrapped value of an `Option<T>` or `Result<T, E>` when present, and falls back to the right-hand expression when absent (`None`) or failed (`Err`).

```neuro
val name   = user.display_name ?? "anonymous"
val config = load_config()     ?? Config::default()
```

**Associativity**: right-to-left. `a ?? b ?? c` parses as `a ?? (b ?? c)`, so each fallback is evaluated only when every left-hand side up to it has produced the absent / error variant. Left-to-right would force the middle fallback even when the chain succeeds early — defeating the short-circuit contract.

**Precedence**: level 14 — looser than `||` (so `a ?? b || c` means `a ?? (b || c)`), tighter than range operators.

**Phase 1.5 status**: the operator is tokenized and parsed today so the precedence and associativity are locked in. Type checking and codegen are deferred to Phase 2, where `Option<T>` and `Result<T, E>` land — until then, using `??` produces:

```
error: operator '??' is not yet supported … requires Option<T> / Result<T, E> — available in Phase 2
```

## Operator Precedence

From highest to lowest (Appendix B):

| Level | Operators | Associativity | Example |
|-------|-----------|---------------|---------|
| 2 | `-` (unary), `!`, `~` | R-to-L | `-x`, `!flag`, `~mask` |
| 3 | `as` | L-to-R | `n as f64` |
| 5 | `*`, `/`, `%` | L-to-R | `a * b`, `n % 2` |
| 6 | `+`, `-` | L-to-R | `a + b`, `x - y` |
| 7 | `<<` | L-to-R | `a << 4` |
| 8 | `&` | L-to-R | `a & mask` |
| 9 | `^` | L-to-R | `a ^ b` |
| 10 | `\|` | L-to-R | `a \| b` |
| 11 | `<`, `>`, `<=`, `>=`, `==`, `!=` | none | `x < y` |
| 12 | `&&` | L-to-R | `a && b` |
| 13 | `\|\|` | L-to-R | `a \|\| b` |
| 14 | `??` | R-to-L | `a ?? b ?? c` parses as `a ?? (b ?? c)` |

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

`%`, `&`, `|`, `^`, `~`, `<<` work only with integer types:
- `i8`, `i16`, `i32`, `i64`
- `u8`, `u16`, `u32`, `u64`

### Comparison Operators

`==`, `!=`, `<`, `>`, `<=`, `>=` work with:
- All numeric types (same type required)
  - *Note:* Float comparison (`f32`, `f64`) utilizes native IEEE-754 ordered predicates. Comparisons involving `NaN` will naturally return `false`.
- `bool` (only `==` and `!=`)
- `string` (only `==` and `!=`) — byte-level equality via length check + `memcmp`

### Logical Operators

`&&`, `||`, `!` work only with `bool`

## Common Patterns

### Range Checking

```neuro
val in_range: bool = x >= min && x <= max
val out_of_range: bool = x < min || x > max
```

### Clamping

> **Note:** `if`-as-expression (assigning the result of an `if` block to a variable) is planned for Phase 1.5 and not yet implemented. Use a `mut` variable and separate assignments for now.

```neuro
// Phase 1.5+ syntax (not yet available):
// val clamped: i32 = if x < min { min } else if x > max { max } else { x }

// Current workaround:
mut clamped: i32 = x
if x < min {
    clamped = min
} else if x > max {
    clamped = max
}
```

### Sign Determination

> **Note:** `if`-as-expression is not yet implemented. See note above.

```neuro
// Phase 1.5+ syntax (not yet available):
// val sign: i32 = if x > 0 { 1 } else if x < 0 { -1 } else { 0 }

// Current workaround:
mut sign: i32 = 0
if x > 0 {
    sign = 1
} else if x < 0 {
    sign = -1
}
```

### Absolute Value

> **Note:** `if`-as-expression is not yet implemented. See note above.

```neuro
// Phase 1.5+ syntax (not yet available):
// val abs: i32 = if x >= 0 { x } else { -x }

// Current workaround:
mut abs: i32 = x
if x < 0 {
    abs = -x
}
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
