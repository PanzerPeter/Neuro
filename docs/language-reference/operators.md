# Operators

Operators perform operations on values (operands). Every built-in operator also takes a
borrowed operand (`&T` or `&mut T`) and reads through it; see
[Reading through a borrow](types.md#reading-through-a-borrow).

## Arithmetic Operators

### Addition (`+`)

```neuro
val sum: i32 = 10 + 20      // 30
val total: f64 = 3.14 + 2.86  // 6.00
```

**Types**: Works with numeric types (integers and floats), and with `string` (concatenation)
**Requirement**: Both operands must be the same type

On two strings, `+` is **concatenation**: it allocates a new owned, immutable `string`
holding the left operand's bytes followed by the right operand's. A `&string` slice may stand in
for either side. Operands are read, not consumed, so they remain usable afterward.

```neuro
val greeting: string = "Hello, " + "Neuro!"   // "Hello, Neuro!"
val a: string = "ab"
val b: string = "cd"
val joined: string = a + &b                   // "abcd"; a and b still valid
```

> The concatenated buffer is heap-allocated and released by `Drop`: with the binding that
> holds it, or, for an anonymous result, at the consumer that reads and discards it (the left
> operand of `a + b + c`, an `==` operand, a `println` argument). Reassigning a binding frees the
> buffer it displaces, and `s = s + "!"` is safe because the new buffer is built before the old
> one is released. The few shapes the compiler cannot prove an owner for are listed in the
> [memory model](memory-model.md#what-still-leaks).

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

### On tensors: element-wise, with broadcasting

All five arithmetic operators apply to tensors. They combine the operands element by
element and allocate a **fresh** tensor, so `*` is the element-wise product; the matrix
product is `@` below. Both owned and borrowed operands are accepted: an owned one is moved,
a borrowed one is only read.

```neuro
val m: Tensor<i32, [2, 3]> = [[1, 2, 3], [4, 5, 6]]
val row: Tensor<i32, [3]> = [100, 200, 300]

val doubled = &m * 2          // a scalar stretches across every element
val shifted = &m + &row       // a lower-rank operand repeats across the leading axes
```

Shapes broadcast: they align at the trailing axis, an extent of `1` stretches across a
wider one, and a lower-rank operand supplies the innermost axes. A scalar sits on either
side and takes the tensor's element type. The full rule, its edge cases, and the way
compound assignment inherits it are in
[Tensors: element-wise arithmetic](tensors.md#element-wise-arithmetic).

### Matrix Multiplication (`@`)

`@` is the matrix product, and it is the one tensor operator that is not element-wise: it
**contracts** an axis instead of walking one. Element `[i, j]` of the result is the dot
product of row `i` of the left operand and column `j` of the right.

```neuro
val a: Tensor<i32, [2, 3]> = [[1, 2, 3], [4, 5, 6]]
val b: Tensor<i32, [3, 2]> = [[7, 8], [9, 10], [11, 12]]

val c = &a @ &b               // [2, 2]: the inner 3 is contracted away
```

**Shape rule**: `[M, K] @ [K, N]` gives `[M, N]`. The two inner extents must agree, and a
mismatch is a compile error. Both operands are rank 2 and neither broadcasts: there is no
vector form and no scalar form of `@`. A `?` extent is rejected on either operand, because
the contracted axis bounds the sum and the result's own two size its buffer.

Shape parameters make the check generic: the repeated `K`
below is verified once at the declaration rather than per call site:

```neuro
func project<M, N, K>(w: &Tensor<f32, [M, K]>, x: &Tensor<f32, [K, N]>) -> Tensor<f32, [M, N]> {
    return w @ x
}
```

**Precedence**: tighter than `*` and `+` and looser than `as` (see
[Operator Precedence](#operator-precedence)), matching
mathematical convention: `a @ b + c` adds `c` to the product, and `a @ b * s` scales it.

On a user type, `@` dispatches through the `MatMul` trait like every other overloadable
operator. See [Tensors: matrix multiplication](tensors.md#matrix-multiplication).

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
**Chaining**: Comparison operators cannot be chained. `a < b < c` is a compile error; write `a < b && b < c` instead.

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

On an integer, `-x` is `0 - x` and follows the same overflow rule as the subtraction: it
panics in debug builds and wraps in release ones. That makes it an overflow at a signed
type's `MIN`, and at every nonzero value of an unsigned type. See
[integer overflow](types.md#integer-overflow).

### Logical NOT (`!`)

```neuro
val not_true: bool = !true  // false
val not_flag: bool = !flag
```

**Types**: Works with boolean type only
**Returns**: boolean

## Bitwise Operators

Work with integer values only (`i8` to `i64`, `u8` to `u64`). Cannot be used with floats or bools.

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
**Note**: Right shift is exposed as the `.shr(n)` method, not an operator (`ashr` for signed receivers, `lshr` for unsigned). See [types.md](types.md#integer-methods).

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

**Types**: between any two numeric types, from `bool` to an integer, and between `char` and an
integer.
**Rules**:
- Widening integers zero-extends (unsigned) or sign-extends (signed).
- Floats to integers truncate towards zero and saturate at the target's range; see
  [Float to integer](types.md#float-to-integer).
- Booleans to integers map `false → 0` and `true → 1`. Nothing casts *to* `bool`.
- `char` to an integer gives its code point, and an integer to `char` the reverse.

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

Shorthand for updating a place. For every type except a tensor each form is
equivalent to a plain assignment with the corresponding binary operator on the right-hand
side; a tensor updates its buffer in place instead (see below).

| Operator | Equivalent to | On a tensor    |
|----------|---------------|----------------|
| `x += n` | `x = x + n`   | `add_assign`   |
| `x -= n` | `x = x - n`   | `sub_assign`   |
| `x *= n` | `x = x * n`   | `mul_assign`   |
| `x /= n` | `x = x / n`   | `div_assign`   |
| `x %= n` | `x = x % n`   | `rem_assign`   |

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

**Requirement**: Left-hand side must be a writable place (see below)
**Type checking**: Same rules as the underlying binary operator apply

### Places: what may sit on the left

Plain and compound assignment take the same left-hand side, a **place**: the storage a
value is written into, rather than a value. A place is rooted at a binding and reached
through any number of projections.

| Form | Example |
|------|---------|
| A binding | `x = 1` |
| A struct field, at any depth | `p.x += 1`, `self.inner.count = 0` |
| An array, slice, or `Vec` element | `arr[i] -= 5`, `xs[0] = 9` |
| An element of a nested array | `grid[row][column] += 1` |
| A field of an element | `cells[i].load *= 2` |
| A tensor coordinate | `t[row, column] = v` |
| The referent of a mutable reference | `*r += 1` |

```neuro
struct Point { x: i32, y: i32 }

impl Point {
    func translate(&mut self, dx: i32, dy: i32) {
        self.x += dx
        self.y += dy
    }
}
```

The rules:

- The binding the place is rooted at must be `mut`, except where the place is reached
  through a borrow. Then the borrow nearest the place decides, however deep the place is:
  `xs: &mut [T]` is an immutable binding you may write elements through (`xs[0][1] = 9`
  included), and a `&[T]` binding declared `mut` is one you may not.
- The value is evaluated before the place is addressed, so a value that grows the `Vec`
  it is stored into (`v[0] = { v.push(x); 42 }`) lands in the grown `Vec`.
- Anything that is not one of the forms above is not a place. A literal or an operator
  result is a value with no storage, and assigning to one is a parse error naming what a
  place is. A projection of a call result (`make()[0] = 5`) parses, and is refused as a
  write into a temporary.
- A tensor index that leaves an axis standing (`t[0, ..]`) produces a *fresh* tensor
  rather than naming storage, so it cannot be assigned to. Name every axis.
- A tuple element (`pair.0 = v`) is not yet a place.

### On tensors: in place, no reallocation

A tensor is the one type whose compound assignment does *not* desugar. `w -= g` updates
the buffer `w` already owns, element by element:

```neuro
mut w: Tensor<f32, [2, 2]> = [[1.0, 2.0], [3.0, 4.0]]
val g: Tensor<f32, [2, 2]> = [[0.5, 0.5], [0.5, 0.5]]

w -= &g          // in place: same buffer, same address, nothing allocated
w *= &g
```

Why it is a separate rule rather than an optimization of the desugaring: `w = w - g`
allocates a fresh tensor and rebinds the name to it on every step, which invalidates any
pointer the runtime, an optimizer's state, or a foreign DLPack consumer is holding, and
costs a whole weight buffer per training step. The in-place form guarantees neither
happens: the tensor's DLPack handle and its `data` pointer are unchanged across the
statement.

The rules:

- The target must be a writable tensor place: a `mut` binding, or a field of one.
- The operand is a tensor of the **same** element type, either owned or borrowed.
  `w += g` consumes `g`; `w += &g` reads it, so one gradient can serve every iteration of
  a loop. Shapes broadcast exactly as they do for the by-value operators, with one
  asymmetry: the result goes back into the target's own buffer, so an operand may be
  stretched **up to** the target's shape and never past it. A shape that does not is a
  compile error naming both types. A scalar operand is accepted the same way, so
  `w *= 2.0` is the scalar broadcast.
- The right-hand side is evaluated **first**, before the target is borrowed for the
  update.
- The element type must have arithmetic: any integer or float, the half-precision `f16` /
  `bf16` included (each element is computed in `f32` and rounded back). `bool` is rejected.
- Element arithmetic carries the scalar guards: an overflowing element panics in debug
  builds, and a zero divisor panics in every build.

## Null/Error Coalescing Operator (`??`)

`??` is the read-site equivalent of `unwrap_or(default)`: it returns the unwrapped value of an `Option<T>` or `Result<T, E>` when present, and falls back to the right-hand expression when absent (`None`) or failed (`Err`). The expression's type is the unwrapped payload `T`.

```neuro
val present = lookup(1) ?? 0        // the Some payload
val absent  = lookup(7) ?? 5        // 5, the fallback

val ok     = divide(24, 4) ?? 0     // the Ok payload
val failed = divide(1, 0)  ?? 4     // 4, the Err payload is discarded
```

For a `Result`, the error payload is **discarded**: `??` states "I do not care why it failed, use this instead". When the reason matters, `match` on the value instead.

**Laziness**: the fallback is only evaluated when the left-hand side is absent or failed. A fallback that calls a function, panics, or does real work is skipped entirely when the value is present.

**Fallback type**: the right-hand side must produce the payload type `T`, not another `Option`/`Result`. A mismatch is an ordinary type error.

**Associativity**: right-to-left. `a ?? b ?? c` parses as `a ?? (b ?? c)`, so each fallback is evaluated only when every left-hand side up to it has produced the absent / error variant. Left-to-right would force the middle fallback even when the chain succeeds early, defeating the short-circuit contract. In a chain, every operand but the last must itself be fallible:

```neuro
val chained = lookup(7) ?? lookup(1) ?? 99
```

**Precedence**: looser than `||` (so `a ?? b || c` means `a ?? (b || c)`), tighter than range operators.

Applying `??` to anything that is not an `Option<T>` or `Result<T, E>` is rejected:

```text
error: `??` expects an `Option<T>` or `Result<T, E>` on the left, found i32
```

`??` moves its left operand, so an `Option` holding an owner (a `string`, a `Vec`) is spent by it, exactly as by a `match`. Runnable program: [`examples/operators/null_coalesce.nr`](../../examples/operators/null_coalesce.nr).

## Error Propagation Operator (`?`)

`?` is the postfix complement of `??`: instead of supplying a fallback, it hands the failure to the caller. `expr?` evaluates to the unwrapped payload when `expr` is `Some` / `Ok`, and otherwise leaves the enclosing function immediately, carrying the failure variant on.

```neuro
func quarter(n: i32) -> Result<i32, i32> {
    val half = halve(n)?     // Err(n) leaves quarter() right here
    val rest = halve(half)?
    Result::Ok(rest)
}
```

It desugars to exactly this `match`:

```neuro
val half = match halve(n) {
    Result::Ok(v)  => v,
    Result::Err(e) => return Result::Err(e)
}
```

**Enclosing function**: the function containing the `?` must return the same fallible enum: a `Result` propagates only out of a `-> Result<_, _>` function, an `Option` only out of a `-> Option<_>` one. Otherwise the failure has nowhere to go:

```text
error: `?` on a Option<i32> has nowhere to propagate: the enclosing function returns i32
```

**No conversion**: the error travels as-is. There is no `From`/`Into` trait system, so the callee's `E` must already be the caller's `E`; a mismatch is an ordinary type error. When the types differ, convert first with a `match` that rebuilds the `Err`; `.map_err` is not available yet.

**Payload types are independent**: only the error types must agree. `?` on a `Result<bool, E>` inside a `-> Result<i32, E>` function is fine: the unwrapped `bool` is used locally, not returned.

**Short-circuiting**: nothing after a failing `?` runs, including the rest of a loop body: `?` returns from the *function*, not from the iteration.

**Precedence**: postfix, binding as tightly as a call or index. `f(x)? + 1` adds to the unwrapped payload, and `parse(s)?.field` reads a field of the unwrapped value.

Runnable program: [`examples/operators/error_propagation.nr`](../../examples/operators/error_propagation.nr).

When the reason for a failure should be handled rather than forwarded, use [`match`](control-flow.md), `??` for a fallback, or [`val-else`](control-flow.md#val-else-unwrap-or-leave-the-scope) to unwrap or leave the scope.

## Pipeline Operator (`|>`)

`x |> f` is `f(x)`. It exists so a chain of transformations reads top to bottom in the order the stages run, rather than inside out:

```neuro
val result = clamp_low(normalize(reading))      // read from the inside out

val result = reading                            // the same thing, read downwards
    |> normalize
    |> clamp_low
```

The piped value becomes the **first** argument of the function on the right, so a stage taking further arguments needs one of the spellings below rather than a partially applied call.

**The right-hand side is a function value, never a call.** Three spellings produce one:

```neuro
val a = reading |> normalize                       // a function name
val b = reading |> scaler.apply                    // a bound method: scaler.apply(reading)
val c = reading |> (|v: i32| -> i32 { v * v })     // a parenthesized closure literal
```

A binding of function type is a name like any other, so `val halve = |v: i32| -> i32 { v / 2 }` then `x |> halve` works too. Anything else is rejected where it is written:

```text
error: the right of `|>` must be a function value: a function name, a bound method
       `receiver.method`, or a parenthesized closure `(|x: T| ...)`
```

**Associativity**: left to right. `x |> f |> g` is `g(f(x))`, so each stage sees what the previous one produced.

**Precedence**: the loosest binary operator in the language, so the whole expression on its left is what gets piped. `100 + 45 |> normalize` normalizes 145. Parenthesize when only part of the expression should flow: `100 + (45 |> normalize)`.

**Line breaks**: a chain may put each `|>` at the start of its own line, as above. The operator continues the previous line, so no trailing marker is needed.

**Ownership**: a stage takes its argument by value, so an owned `string`, a `Vec` or a tensor flows through a chain as readily as a scalar does; each intermediate is released once the stage after it has consumed it.

Runnable program: [`examples/operators/pipeline.nr`](../../examples/operators/pipeline.nr).

## Function Composition (`>>`)

`f >> g` is the function `|x| g(f(x))`. Where `|>` pushes a value through a chain, `>>` names the chain itself, so one preparation pipeline can be written once and applied wherever it is needed:

```neuro
val prepare = normalize >> clamp_low          // a value of type (i32) -> i32

val ready = prepare(reading)                  // applied here
val batch = other |> prepare                  // and here
```

**Both operands are function names.** A composition calls each stage directly, so the operand is a `func` in scope, not a value holding one:

```text
error: `>>` composes named functions: 'held' is a binding, so name the `func` it holds instead
```

A closure literal, a bound method `receiver.method`, an associated path `Type::member` and a generic function are all rejected the same way. Each stage takes exactly one parameter, and each stage's result type must be what the next one accepts:

```text
error: 'label' returns string, which 'double' cannot take: it expects i32
```

The types need not stay the same along the chain: `label >> width` composes `(i32) -> string` with `(string) -> i32` into `(i32) -> i32`.

**The result is an ordinary function value.** It binds to a `val`, passes to a `(T) -> U` parameter, and is called as many times as needed.

**Associativity**: left to right. `f >> g >> h` applies `f` first and `h` last.

**Precedence**: tighter than `|>`, looser than everything else. That is what makes `x |> f >> g` apply the composed function to `x` rather than compose `x |> f` with `g`. A composition called where it is written needs no name: `(f >> g)(x)`.

**`>>` is not right shift.** Shifting right is the `.shr(n)` integer method, which leaves the token for the operator an AI-first language reaches for far more often. It is not a token of its own either: the parser reads two adjacent `>`, so the closing brackets of `Vec<Vec<i32>>` are unaffected.

Runnable program: [`examples/operators/compose.nr`](../../examples/operators/compose.nr).

## Operator Precedence

From highest to lowest, matching the parser's Pratt ladder:

| Level | Operators | Associativity | Example |
|-------|-----------|---------------|---------|
| 19 (highest) | `.` | L-to-R | `p.x` |
| 18 | call `f(…)`, index `a[i]`, postfix `?`, turbofish `::<…>` | L-to-R | `f(x)?`, `arr[i]` |
| 17 | `-` (unary), `!`, `~` | R-to-L | `-x`, `!flag`, `~mask` |
| 16 | `as` | L-to-R | `n as f64` |
| 15 | `@` | L-to-R | `w @ x` |
| 14 | `*`, `/`, `%` | L-to-R | `a * b`, `n % 2` |
| 13 | `+`, `-` | L-to-R | `a + b`, `x - y` |
| 12 | `<<` | L-to-R | `a << 4` |
| 11 | `<`, `>`, `<=`, `>=` | L-to-R | `x < y` |
| 10 | `==`, `!=` | L-to-R | `x == y` |
| 9 | `&` | L-to-R | `a & mask` |
| 8 | `^` | L-to-R | `a ^ b` |
| 7 | `\|` | L-to-R | `a \| b` |
| 6 | `&&` | L-to-R | `a && b` |
| 5 | `\|\|` | L-to-R | `a \|\| b` |
| 4 | `??` | R-to-L | `a ?? b ?? c` parses as `a ?? (b ?? c)` |
| 3 | `..`, `..=` | L-to-R | `1..=n` |
| 2 | `>>` | L-to-R | `f >> g >> h` applies `f` first |
| 1 (lowest) | `\|>` | L-to-R | `x \|> f \|> g` parses as `g(f(x))` |

Comparison binds tighter than equality: `x < y == z` parses as `(x < y) == z`. `>>` composes
functions rather than shifting bits; right shift is the `.shr(n)` method.

### Precedence Examples

```neuro
a + b * c       // Same as: a + (b * c)
a * b + c       // Same as: (a * b) + c
a < b == c < d  // Same as: (a < b) == (c < d)
w @ x + b       // Same as: (w @ x) + b
w @ x * s       // Same as: (w @ x) * s
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

`+` additionally works on `string` (and `&string`) as concatenation, producing a new owned
`string`. The other arithmetic operators have no string meaning.

### Integer-Only Operators

`%`, `&`, `|`, `^`, `~`, `<<` work only with integer types:
- `i8`, `i16`, `i32`, `i64`
- `u8`, `u16`, `u32`, `u64`

### Comparison Operators

`==`, `!=`, `<`, `>`, `<=`, `>=` work with:
- All numeric types (same type required)
  - *Note:* Float comparison (`f32`, `f64`) utilizes native IEEE-754 ordered predicates. Comparisons involving `NaN` will naturally return `false`.
- `bool` (only `==` and `!=`)
- `char`, which has a built-in total order over its Unicode scalar values
- `string` (only `==` and `!=`), byte-level equality via length check + `memcmp`
- a newtype over any of the above, which compares as its inner type does

Nothing else has comparison built in. A struct, enum or newtype gets `==` / `!=` from
`impl PartialEq` and the ordering operators from `impl Comparable` (see
[Operator Overloading](#operator-overloading)), and a struct can also `@derive(PartialEq)`.
Arrays, tuples and collections have no equality yet, and neither does a reference to
anything but a string. Read through it with `*` first.

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
val clamped: i32 = if x < min { min } else if x > max { max } else { x }
```

### Sign Determination

```neuro
val sign: i32 = if x > 0 { 1 } else if x < 0 { -1 } else { 0 }
```

### Absolute Value

```neuro
val abs: i32 = if x >= 0 { x } else { -x }
```

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

## Operator Overloading

Operators on a custom type are sugar for method calls. Implement the corresponding
**operator trait** to make an operator work on your type. The operator traits are
built into the compiler: you write only the `impl`, never a `trait` declaration.

An arithmetic, bitwise, or unary operator trait declares its result type with
`type Output = T`:

```neuro
@derive(Copy, Clone)
struct Vec2 { x: i32, y: i32 }

impl Add for Vec2 {
    type Output = Vec2
    func add(self, rhs: Vec2) -> Vec2 {
        Vec2 { x: self.x + rhs.x, y: self.y + rhs.y }
    }
}

impl Neg for Vec2 {
    type Output = Vec2
    func neg(self) -> Vec2 { Vec2 { x: -self.x, y: -self.y } }
}

val c = Vec2 { x: 1, y: 2 } + Vec2 { x: 3, y: 4 }   // (4, 6), via Add::add
val d = -c                                          // (-4, -6), via Neg::neg
```

Comparison uses `PartialEq` (equality) and `Comparable` (ordering); their methods take
`&self` and `rhs: &Self` and return `bool`. `Comparable` requires `PartialEq` on the
same type:

```neuro
impl PartialEq for Vec2 {
    func eq(&self, rhs: &Vec2) -> bool { self.x == rhs.x && self.y == rhs.y }
    func ne(&self, rhs: &Vec2) -> bool { self.x != rhs.x || self.y != rhs.y }
}

val a = Vec2 { x: 1, y: 2 }
val b = Vec2 { x: 1, y: 2 }
if a == b { }   // via PartialEq::eq
```

**Operator → trait → method:**

| Operator(s) | Trait | Method(s) |
|---|---|---|
| `+` | `Add` | `add` |
| `-` (binary) | `Sub` | `sub` |
| `*` | `Mul` | `mul` |
| `/` | `Div` | `div` |
| `%` | `Rem` | `rem` |
| `-a` | `Neg` | `neg` |
| `~a` | `Not` | `not` |
| `&` `\|` `^` `<<` | `BitAnd` `BitOr` `BitXor` `Shl` | `bitand` `bitor` `bitxor` `shl` |
| `@` | `MatMul` | `matmul` |
| `==` `!=` | `PartialEq` | `eq` `ne` |
| `<` `<=` `>` `>=` | `Comparable` | `lt` `le` `gt` `ge` |

Rules and limits:

- The receiver type must be `Copy` (the scalar path). Each operator dispatches to its own
  method; implement the method for every operator you use.
- A declared `type Output` must match the method's return type.
- The logical `!a` (boolean NOT) is **not** overloadable: it is always boolean negation.
- Compound assignment (`v += w`) works when the type implements the matching by-value
  operator: it desugars to `v = v + w`. In-place `*Assign` behaviour is compiler-known on
  tensors (see [Compound Assignment Operators](#compound-assignment-operators)) but not
  yet declarable for a user type. The tensor arithmetic operators are compiler-known too,
  and are not reached through an operator-trait impl.
- Operator overloading is fully monomorphized and erased: each operator becomes the
  method call it stands for, with no vtable and no runtime cost.

See [`examples/operators/operator_overloading.nr`](../../examples/operators/operator_overloading.nr)
for a complete program.

## References

- [Types](types.md): type requirements for operators
- [Expressions](expressions.md): operator precedence and evaluation
- [Variables](variables.md): assignment operator
