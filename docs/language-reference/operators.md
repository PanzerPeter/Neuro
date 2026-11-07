# Operators

Operators combine values into new values, or write a new value into a place. This page lists each operator's operand types and the rules that differ from Rust. Built-in operators on numbers, `bool` and `char` also read through a borrow, so `x + 1` works the same for `x: &i32` (see [Reading through a borrow](ownership.md#reading-through-a-borrow)).

## Arithmetic

| Operator | Meaning | Operands | Notes |
|----------|---------|----------|-------|
| `a + b` | Addition | Two numbers of one type, or two strings | On strings, concatenation |
| `a - b` | Subtraction | Two numbers of one type | |
| `a * b` | Multiplication | Two numbers of one type | |
| `a / b` | Division | Two numbers of one type | Integer division truncates |
| `a % b` | Remainder | Two numbers of one type | Integers and floats |
| `a @ b` | Matrix product | Two rank-2 tensors | Contracts an axis; see [Tensors](#tensors) |

Both operands must have the same type. A bare literal takes the other operand's type, so `x + 1` works for `x: i64`, but two typed values never convert implicitly. `x + y` with `x: i32` and `y: f64` fails with `type mismatch: expected i32, found f64`, so convert with `as` first.

```neuro
val sum: i32 = 10 + 20                         // 30
val ratio: f64 = 10.0 / 4.0                    // 2.5
val greeting: string = "Hello, " + "Neuro!"   // "Hello, Neuro!"
```

On two strings, `+` allocates a new owned `string`. Either side may be a `&string`. The operands are read, not moved, so `a` and `b` stay usable after `a + b`. The few shapes whose owner the compiler cannot track are listed in the [memory model](memory-model.md#what-still-leaks).

### Integer division and remainder

Integer `/` truncates toward zero, and `%` takes the sign of its left operand. So `(a / b) * b + a % b` always equals `a`.

```neuro
val q: i32 = -7 / 2   // -3, not -4
val r: i32 = -7 % 3   // -1
val s: i32 = 7 % -3   // 1
```

Dividing or taking a remainder by zero panics in every build, `-O0` through `-O3`:

```text
panic: division by zero at main.nr:4:18
panic: remainder by zero at main.nr:4:18
```

### Float division and remainder

Float `/` never panics. A zero divisor gives an infinity or NaN. Float `%` takes the sign of its left operand, like `fmod`, so `-5.5 % 2.0` is `-1.5`.

### Overflow

`+`, `-`, `*`, unary `-`, `/`, `%` and shifts are checked. In a debug build (`-O0`, the default) an overflow panics. In a release build (`-O1` to `-O3`) it wraps in two's complement:

```text
panic: integer overflow at main.nr:3:9
```

Division and remainder overflow only at `MIN / -1` and `MIN % -1`. The full rule, including how `const` initializers are checked, is in [integer overflow](types.md#integer-overflow).

### Tensors

The five arithmetic operators apply to tensors element by element and allocate a fresh tensor, so `*` is the element-wise product. An owned operand is moved, and a borrowed one is only read.

```neuro
val m: Tensor<i32, [2, 3]> = [[1, 2, 3], [4, 5, 6]]
val row: Tensor<i32, [3]> = [100, 200, 300]

val doubled = &m * 2          // a scalar stretches across every element
val shifted = &m + &row       // the rank-1 operand repeats across the rows
```

Shapes align at the trailing axis. An extent of `1` stretches across a wider one, and a lower-rank operand supplies the innermost axes. A pair that does not fit fails with ``the operands of `+` do not broadcast``. The full rule is in [Element-wise arithmetic](tensors.md#element-wise-arithmetic).

`@` is the matrix product, and the one tensor operator that contracts an axis instead of walking one. For `[M, K] @ [K, N]` the result is `[M, N]`, and element `[i, j]` is the dot product of row `i` of the left operand and column `j` of the right. Both operands must be rank 2, and neither broadcasts, so there is no vector or scalar form. A `?` extent on either operand is a compile error, because the contracted axis must be known.

```neuro
val a: Tensor<i32, [2, 3]> = [[1, 2, 3], [4, 5, 6]]
val b: Tensor<i32, [3, 2]> = [[7, 8], [9, 10], [11, 12]]

val c = &a @ &b               // [2, 2]
```

```text
error: the operands of `@` do not multiply: &Tensor<i32, [2, 3]> against &Tensor<i32, [2, 2]>; matrix multiplication takes two rank-2 tensors whose inner axes agree, `[M, K] @ [K, N]` giving `[M, N]`
```

Shape parameters make the check generic. The repeated `K` below is verified once, at the declaration:

```neuro
func project<M, N, K>(w: &Tensor<f32, [M, K]>, x: &Tensor<f32, [K, N]>) -> Tensor<f32, [M, N]> {
    return w @ x
}
```

On a user type, `@` dispatches through the `MatMul` trait (see [Operator Overloading](#operator-overloading)).

## Comparison

Comparisons return `bool`.

| Operator | Meaning | Operands |
|----------|---------|----------|
| `==`, `!=` | Equal, not equal | Numbers, `bool`, `char`, `string` (and `&string`), a newtype over these, or a type with `impl PartialEq` |
| `<`, `<=`, `>`, `>=` | Ordering | Numbers and `char` (by code point), or a type with `impl Comparable` |

- Both operands must have the same type. `a == b` with `i32` and `i64` fails with `type mismatch: expected i32, found i64`.
- Comparisons do not chain. `a < b < c` fails with ``comparison operators cannot be chained: use `&&` to combine separate comparisons``. Write `a < b && b < c`.
- Ordering is not defined for `string` or `bool`. `"a" < "b"` fails with `cannot apply binary operator < to types string and string`.
- Floats follow IEEE ordering: every comparison involving NaN is `false`, `!=` included. So `nan != nan` is `false`.
- Arrays, tuples and collections have no `==` yet. `[1, 2] == [1, 2]` fails with `cannot apply binary operator == to types [i32; 2] and [i32; 2]`. Compare the elements.
- Comparisons read through `&` and `&mut`, so `r == 4` works for `r: &i32`. On two references to a struct, `==` calls the struct's `eq`, as `*ra == *rb` does.

## Logical

| Operator | Meaning | Operands | Notes |
|----------|---------|----------|-------|
| `a && b` | And | `bool` | Short-circuit: `b` runs only when `a` is `true` |
| `a \|\| b` | Or | `bool` | Short-circuit: `b` runs only when `a` is `false` |
| `!a` | Not | `bool` | Never overloadable |

With `false && f()`, `f` is never called. An integer is not a condition: `x && true` with `x: i32` fails with `cannot apply binary operator && to types i32 and bool`.

## Unary

| Operator | Meaning | Operands | Notes |
|----------|---------|----------|-------|
| `-a` | Negation | A number | Same type. Overflows where `0 - a` does |
| `~a` | Bitwise not | An integer, or a user type with `impl BitNot` | Same type. `~0` is `-1` for `i32` |

`-a` is `0 - a`, so it overflows at a signed type's `MIN` and at every nonzero unsigned value. A negative literal is checked as written instead, so `val x: u32 = -5` fails with ``literal -5 is negative and u32 is unsigned: use a signed type, or `0u32.wrapping_sub(5u32)` if the two's-complement wrap was intended``. `~` on a `bool` fails with `cannot apply operator ~ to type bool`.

## Bitwise and Shifts

| Operator | Meaning | Operands |
|----------|---------|----------|
| `a & b` | And | Two integers of one type |
| `a \| b` | Or | Two integers of one type |
| `a ^ b` | Xor | Two integers of one type |
| `a << n` | Left shift | Integer `a`, and `n` of the same type (a literal adapts) |

Bitwise operators take integers only (`i8` to `i64`, `u8` to `u64`), and the result has the operand type.

A shift amount outside `0..bits`, or a negative one, is an overflow. A debug build panics with `shift overflow`. A release build masks the amount to `amount & (bits - 1)`:

```neuro
mut s: i32 = 40
val r: i32 = 1 << s   // debug: panic: shift overflow; -O1 and up: 1 << 8 = 256
```

There is no right-shift operator. `>>` is [function composition](#function-composition-), so `x >> 1` fails to compile with ``the operands of `>>` must be function values``. Use `x.shr(1)`, which is arithmetic on signed integers and logical on unsigned ones (see [integer methods](types.md#integer-methods)).

## Assignment

`=` writes a value into a place. The place must be `mut`, and the value must have the place's type:

```neuro
mut x: i32 = 10
x = x + 5
```

Assigning to an immutable binding fails with `cannot assign to immutable variable 'x'`.

### Compound Assignment Operators

`x op= n` is `x = x op n` unless the target updates in place: a tensor does (see below), and so does a user type with the matching [`*Assign` impl](#operator-overloading). The forms are `+=`, `-=`, `*=`, `/=`, `%=`, `&=`, `|=`, `^=` and `<<=`. There is no `>>=`, because `>>` is composition. A tensor target takes only the arithmetic forms.

```neuro
mut score: i32 = 100
score += 50    // 150
score %= 13    // 11
```

### Places: what may sit on the left

Plain and compound assignment take a **place**: the storage a value is written into. A place is rooted at a binding and reached through any number of projections.

| Form | Example |
|------|---------|
| A binding | `x = 1` |
| A struct field, at any depth | `p.x += 1`, `self.inner.count = 0` |
| A tuple element | `pair.0 = 7`, `t.0.1 += 1` |
| An array, slice, or `Vec` element | `arr[i] -= 5`, `xs[0] = 9` |
| An element of a nested array | `grid[row][column] += 1` |
| A field of an element | `cells[i].load *= 2` |
| A tensor coordinate | `t[row, column] = v` |
| The referent of a mutable reference | `*r += 1` |

The rules:

- The binding a place is rooted at must be `mut`, unless the place is reached through a borrow. Then the borrow nearest the place decides: `xs: &mut [T]` lets you write `xs[0][1] = 9` through an immutable binding, and a `mut` binding of type `&[T]` does not let you write.
- The value on the right is evaluated before the place is addressed. A block that pushes onto the `Vec` it is stored into, then yields `42`, writes into the grown `Vec`.
- Anything else is not a place. A literal or operator result is a parse error (``the left of `=` must be a place``). A call result such as `make()[0] = 5` parses, then fails with `cannot assign into a temporary`.
- A tensor index that leaves an axis standing (`t[0, ..]`) makes a fresh tensor, so it cannot be assigned to. Name every axis.
- A newtype's `.0` reads its inner value but is not a place. Rebuild the value instead: `m = Meters(v)`.

### Tensors: in place

A tensor's compound assignment does not desugar to `w = w - g`. It updates the buffer `w` already owns, element by element:

```neuro
mut w: Tensor<f32, [2, 2]> = [[1.0, 2.0], [3.0, 4.0]]
val g: Tensor<f32, [2, 2]> = [[0.5, 0.5], [0.5, 0.5]]

w -= &g          // same buffer, same address, nothing allocated
w *= 2.0         // a scalar is broadcast
```

The rewrite would allocate a new tensor on every step, which invalidates any pointer the runtime, an optimizer or a DLPack consumer holds, and costs a whole weight buffer per training step. The in-place form does neither, so the tensor's DLPack handle and `data` pointer are unchanged across the statement.

- The target must be a `mut` tensor binding, or a field of one.
- The operand is a tensor of the **same** element type, owned or borrowed. `w += g` moves `g`, and `w += &g` reads it, so one gradient can serve every iteration of a loop.
- Shapes broadcast as they do for the by-value operators, with one asymmetry: the result goes back into the target, so the operand may stretch **up to** the target's shape and never past it.
- The right-hand side is evaluated first, before the target is borrowed.
- The element type must have arithmetic: any integer or float, including `f16` and `bf16` (computed in `f32`, rounded back). `bool` is rejected.
- Element arithmetic keeps the scalar guards: an overflowing element panics in a debug build, and a zero divisor panics in every build.

## Operator Overloading

An operator on a user type is sugar for a trait method call. Implement the matching **operator trait** to make the operator work on your type. The traits are built into the compiler, so you write only the `impl`, never a `trait` declaration.

An arithmetic, bitwise or unary trait declares its result with `type Output`:

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

func main() -> i32 {
    val c = Vec2 { x: 1, y: 2 } + Vec2 { x: 3, y: 4 }   // via Add::add
    val d = -c                                          // via Neg::neg
    return d.x
}
```

`==` and `!=` use `PartialEq`, and the orderings use `Comparable`. `Comparable` requires `PartialEq` on the same type. A type that implements `Comparable` without `PartialEq` fails with:

```text
error: `impl Comparable for Meters` requires `impl PartialEq for Meters`
```

| Operator(s) | Trait | Method(s) |
|-------------|-------|-----------|
| `+` | `Add` | `add` |
| `-` (binary) | `Sub` | `sub` |
| `*` | `Mul` | `mul` |
| `/` | `Div` | `div` |
| `%` | `Rem` | `rem` |
| `-a` | `Neg` | `neg` |
| `~a` | `BitNot` | `bitnot` |
| `&` `\|` `^` `<<` | `BitAnd` `BitOr` `BitXor` `Shl` | `bitand` `bitor` `bitxor` `shl` |
| `@` | `MatMul` | `matmul` |
| `==` `!=` | `PartialEq` | `eq` `ne` |
| `<` `<=` `>` `>=` | `Comparable` | `lt` `le` `gt` `ge` |
| `+=` `-=` `*=` `/=` `%=` | `AddAssign` `SubAssign` `MulAssign` `DivAssign` `RemAssign` | `add_assign` `sub_assign` `mul_assign` `div_assign` `rem_assign` |
| `&=` `\|=` `^=` `<<=` | `BitAndAssign` `BitOrAssign` `BitXorAssign` `ShlAssign` | `bitand_assign` `bitor_assign` `bitxor_assign` `shl_assign` |

The rules:

- Each method has the shape its trait fixes. A by-value trait takes `func add(self, rhs: Rhs) -> Output`, a comparison `func eq(&self, rhs: &Rhs) -> bool`, and a `*Assign` trait `func add_assign(&mut self, rhs: Rhs)`. Any other shape is rejected with `does not match the trait signature`.
- `eq` and `lt` are required. `ne` comes from `eq`, and `le`, `gt` and `ge` from `lt`, so an impl writes those only to replace them. Leaving out a required method fails with `missing required method`.
- The declared `type Output` must match the method's return type.
- `Rhs` defaults to the type itself and is written on the trait: `impl Mul<f32> for Vec2` takes `v * 2.0`. A type has one impl of each trait.
- `!a` (logical not), `&&`, `||`, `??`, `?`, `|>` and `>>` are not overloadable.
- Calls are monomorphized. Each operator becomes the method call it stands for, with no vtable.

The complete program is [`examples/operators/operator_overloading.nr`](../../examples/operators/operator_overloading.nr).

### Operators on Non-`Copy` Types

An operator impl works on any type, and it follows the signature of its method. The by-value traits take both operands by value, so `a + b` on a non-`Copy` type moves `a` and `b`, as the call `a.add(b)` would. Reading either one afterwards is a use of a moved value.

To leave the right operand with its owner, implement the trait with a reference `Rhs`. The operand is then written as a borrow:

```neuro
impl Mul<&Samples> for Samples {
    type Output = Samples
    func mul(self, rhs: &Samples) -> Samples { ... }
}

val scaled = joined * &scale    // moves `joined`, reads `scale`
```

The left operand cannot be borrowed this way, because there is no impl on a reference type. Reaching a non-`Copy` left operand through a borrow fails with `cannot move out of`.

A comparison only reads its operands, so neither moves. The right one may be written bare or borrowed: `a == b` and `a == &b` mean the same.

### In-Place Updates

A type with a `*Assign` impl updates in place, and `x += y` calls `x.add_assign(y)`. That impl wins over `Add` when a type has both. Without one, `x += y` is `x = x + y`, which on a non-`Copy` type moves `x` out and stores the result back.

```neuro
impl AddAssign<i32> for Samples {
    func add_assign(&mut self, rhs: i32) {
        self.values.push(rhs)
    }
}

scaled += 10
```

The target may be any [place](#places-what-may-sit-on-the-left). It is borrowed like a method's `&mut self` receiver, so it must be writable and free of other borrows. The value is evaluated first, so `x += x.doubled()` reads `x` before updating it.

The complete program is [`examples/operators/in_place_operators.nr`](../../examples/operators/in_place_operators.nr).

## Null/Error Coalescing Operator (`??`)

`??` is the read-site equivalent of `unwrap_or(default)`. It returns the payload of an `Option<T>` or `Result<T, E>` when present, and evaluates the right-hand side when the value is `None` or `Err`. The expression has the payload type `T`.

```neuro
val present = lookup(1) ?? 0        // the Some payload
val absent  = lookup(7) ?? 5        // 5, the fallback
```

- **Errors are discarded.** For a `Result`, `??` ignores the `Err` payload. When the reason matters, use `match`.
- **Laziness.** The fallback runs only when the left side is absent or failed.
- **Fallback type.** The right side must produce `T`, not another `Option` or `Result`. A mismatch is an ordinary type error.
- **Borrowed payload.** When the payload is a borrow of a `Copy` type, as `get` returns, and the fallback is a value, `??` reads through: `counts.get(k) ?? 0` is an `i32`.
- **Diverging fallback.** The right side may leave the scope instead of producing a value. `return`, `break`, `continue` and `panic(...)` all work unbraced:

```neuro
val config = load() ?? panic("no config")   // the payload, or stop the program
val first = lookup(key) ?? return 0         // leave the function
val value = parse(line) ?? continue         // skip this iteration
```

- **Associativity.** Right to left: `a ?? b ?? c` is `a ?? (b ?? c)`, so each fallback runs only when every earlier side has failed. Every operand but the last must itself be fallible.
- **Precedence.** Looser than `||`. The fallback takes everything to its right that binds looser than `??`, so `a ?? 0 |> f` is `a ?? (0 |> f)` and `a ?? b || c` is `a ?? (b || c)`. Parenthesize the left side or the fallback to group otherwise.
- **Moves its operand.** An `Option` holding an owner, such as a `string` or a `Vec`, is moved by `??`, as it is by `match`.

Applying `??` to a non-fallible value fails with:

```text
error: `??` expects an `Option<T>` or `Result<T, E>` on the left, found i32: `??` unwraps a fallible value or falls back
```

The runnable program is [`examples/operators/null_coalesce.nr`](../../examples/operators/null_coalesce.nr).

## Error Propagation Operator (`?`)

`?` is the postfix counterpart of `??`. It hands the failure to the caller instead of supplying a fallback. `expr?` yields the payload of `Some` or `Ok`. Otherwise it returns the failure from the enclosing function at once, as `match expr { Ok(v) => v, Err(e) => return Err(e) }` would.

```neuro
func halve(n: i32) -> Result<i32, i32> {
    if n % 2 != 0 { return Result::Err(n) }
    return Result::Ok(n / 2)
}

func quarter(n: i32) -> Result<i32, i32> {
    val half = halve(n)?     // Err(n) leaves quarter() right here
    val rest = halve(half)?
    Result::Ok(rest)
}
```

- **Enclosing function.** It must return the same fallible enum. A `Result` propagates only out of a `-> Result<_, _>` function, and an `Option` only out of a `-> Option<_>` one:

  ```text
  error: `?` on a Option<i32> has nowhere to propagate: the enclosing function returns i32, but it must return an `Option` for the failure to be forwarded; otherwise handle the value with `match`, `??`, or `val-else`
  ```

- **No conversion.** The error travels unchanged, and there is no `From` trait. When the types differ, convert first with `.map_err`: `read(path).map_err(AppError::Io)?`.
- **Payload types are independent.** Only the error types must agree. `?` on a `Result<bool, E>` inside a `-> Result<i32, E>` function is fine, because the `bool` is used locally.
- **Short-circuit.** Nothing after a failing `?` runs, including the rest of a loop body. `?` returns from the function, not from the iteration.
- **Precedence.** Postfix, binding as tightly as a call. `f(x)? + 1` adds to the payload, and `parse(s)?.field` reads a field of it.

The runnable program is [`examples/operators/error_propagation.nr`](../../examples/operators/error_propagation.nr). To handle a failure rather than forward it, use [`match`](control-flow.md#pattern-matching), `??`, or [`val-else`](control-flow.md#val-else-unwrap-or-leave-the-scope).

## Pipeline Operator (`|>`)

`x |> f` is `f(x)`. A chain of stages reads top to bottom in the order it runs. The piped value becomes the first argument of the function on the right:

```neuro
val result = reading
    |> normalize
    |> clamp_low
```

The right side must be a function value, never a call. Five spellings produce one:

```neuro
val a = reading |> normalize                       // a function name
val b = reading |> Scaler::halve                   // an associated function
val c = reading |> convert::<u8>                   // a generic function's instance
val d = reading |> scaler.apply                    // a bound method: scaler.apply(reading)
val e = reading |> (|v| v * v)                     // a parenthesized closure literal
```

A path may also name a non-generic enum's tuple variant, which constructs it (`5 |> Wrapped::Num`). A binding of function type is a name like any other. Anything else fails with:

```text
error: the right of `|>` must be a function value: a function name, a path `Type::function`, an instance `f::<T>`, a bound method `receiver.method`, or a parenthesized closure `(|x| ...)`
```

- **Associativity.** Left to right, so `x |> f |> g` is `g(f(x))`.
- **Precedence.** The loosest binary operator, so the whole expression on its left is piped: `100 + 45 |> normalize` normalizes 145. Parenthesize to pipe part of it: `100 + (45 |> normalize)`.
- **Ownership.** A stage takes its argument by value, so an owned `string`, `Vec` or tensor flows through a chain. Each intermediate is released after the stage that consumes it.

The runnable program is [`examples/operators/pipeline.nr`](../../examples/operators/pipeline.nr).

## Function Composition (`>>`)

`f >> g` is the function `|x| g(f(x))`. Where `|>` pushes one value through a chain, `>>` names the chain, so a preparation pipeline can be written once and applied wherever it is needed:

```neuro
val prepare = normalize >> clamp_low          // a value of type (i32) -> i32

val ready = prepare(reading)
val batch = other |> prepare
```

Both operands are function values, spelled the same ways as a `|>` target. The first operand types the composition. A later stage is called with the output of the stage before it, so generics and closure parameters infer from there. A generic first operand needs a turbofish (`identity::<i32> >> double`). A stage that cannot take the previous stage's output fails at that stage, for example `type mismatch: expected string, found i32` when the output is `i32` and the next stage takes `string`.

- **Associativity.** Left to right: `f >> g >> h` applies `f` first.
- **Precedence.** Tighter than `|>`, looser than everything else, so `x |> f >> g` applies the composed function to `x`. A composition called in place needs no name: `(f >> g)(x)`.
- **Refused operands.** A bound method's receiver must be a variable or a field of one, since a call result cannot be borrowed past its expression. A `move` closure must be bound with `val` first.
- **Not a shift.** `>>` never shifts bits. The right shift is the `.shr(n)` method (see [Bitwise and Shifts](#bitwise-and-shifts)). The parser reads two adjacent `>` tokens, so `Vec<Vec<i32>>` still closes correctly.

The runnable program is [`examples/operators/compose.nr`](../../examples/operators/compose.nr).

## Operator Precedence

From highest to lowest. The parser uses the same levels.

| Level | Operators | Associativity | Example |
|-------|-----------|---------------|---------|
| 17 (highest) | `.` | L-to-R | `p.x` |
| 16 | call `f(…)`, index `a[i]`, postfix `?`, turbofish `::<…>` | L-to-R | `f(x)?`, `arr[i]` |
| 15 | prefix `-`, `!`, `~`, `*`, `&`, `&mut` | R-to-L | `-x`, `!flag`, `~mask`, `*r`, `&mut x` |
| 14 | `as` | L-to-R | `n as f64` |
| 13 | `*`, `/`, `%`, `@` | L-to-R | `a * b`, `n % 2`, `w @ x` |
| 12 | `+`, `-` | L-to-R | `a + b`, `x - y` |
| 11 | `<<` | L-to-R | `a << 4` |
| 10 | `&` | L-to-R | `a & mask` |
| 9 | `^` | L-to-R | `a ^ b` |
| 8 | `\|` | L-to-R | `a \| b` |
| 7 | `<`, `>`, `<=`, `>=`, `==`, `!=` | none (no chaining) | `x < y`, `x == y` |
| 6 | `&&` | L-to-R | `a && b` |
| 5 | `\|\|` | L-to-R | `a \|\| b` |
| 4 | `??` | R-to-L | `a ?? b ?? c` is `a ?? (b ?? c)` |
| 3 | `..`, `..=` | L-to-R | `1..=n` |
| 2 | `>>` | L-to-R | `f >> g >> h` applies `f` first |
| 1 (lowest) | `\|>` | L-to-R | `x \|> f \|> g` is `g(f(x))` |

Three rules are easy to misread:

- `&`, `^` and `|` bind tighter than comparisons, unlike C. So `x & 3 == 2` is `(x & 3) == 2`.
- `as` binds tighter than every binary operator. So `2 + 3 as i32` is `2 + (3 as i32)`, and `n as f64 + 1.0` is `(n as f64) + 1.0`.
- `@` shares the `*` row, so `x * 0.5 @ w` groups left to right, and `a @ b + c` adds `c` to the product.

```neuro
x & mask == 0     // (x & mask) == 0
a + b * c         // a + (b * c)
w @ x + b         // (w @ x) + b
a || b && c       // a || (b && c)
```

## See also

- [Types](types.md): operand types, integer overflow and integer methods
- [Tensors](tensors.md): element-wise arithmetic, `@` and in-place updates
- [Ownership](ownership.md): borrows read through by operators
- [Expressions](expressions.md): what an expression is, and statement boundaries
