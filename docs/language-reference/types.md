# Type System

Neuro is statically typed, and no value changes type without an explicit `as`. Annotations are optional where the initializer's type is unambiguous: literals take their type from context, and every other expression already carries the type the checker resolved for it. Structs and enums, ownership and borrowing, and generics and traits have their own pages, linked where they come up.

## Primitive Types

Text is covered in [Strings](strings.md). The scalar types are below.

### Integer Types

| Type | Bits | Range |
|------|------|-------|
| `i8` | 8 | -128 to 127 |
| `i16` | 16 | -32,768 to 32,767 |
| `i32` | 32 | -2,147,483,648 to 2,147,483,647 |
| `i64` | 64 | -9,223,372,036,854,775,808 to 9,223,372,036,854,775,807 |
| `u8` | 8 | 0 to 255 |
| `u16` | 16 | 0 to 65,535 |
| `u32` | 32 | 0 to 4,294,967,295 |
| `u64` | 64 | 0 to 18,446,744,073,709,551,615 |

**Default type**: an integer literal is `i32` unless context says otherwise. Contextual inference from a declaration, parameter, or return type is implemented, and a literal that does not fit its type is an error. An unannotated literal past the `i32` range is not promoted to `i64`.

Arithmetic made only of literals takes its context's type the same way, so `val d: u8 = 200 + 50` is `u8` arithmetic and `val big: i64 = 2147483647 + 1` is `2147483648`. A literal next to a typed operand takes that operand's type.

**Type suffixes**: a suffix appended directly to an integer literal overrides contextual inference and pins the type:

```neuro
val a = 42i64      // i64, no annotation needed
val b = 255u8      // u8
val c = 0xFFu8     // hex literal with suffix
val d = 0b1010i32  // binary literal with suffix
```

The valid suffixes are `i8` through `i64` and `u8` through `u64`. A value that does not fit the suffix type is a compile error, as `300u8` is.

### Integer Overflow

When a runtime `+`, `-`, `*`, or unary `-` produces a result outside its type's range, the behavior depends on the optimization level, set with `-O` on `neurc run` and `neurc compile`:

| Build | Flag | Overflow behavior |
|-------|------|-------------------|
| Debug | `-O0` (default) | The program panics: `panic: integer overflow at <file>:<line>:<col>`. |
| Release | `-O1`, `-O2`, `-O3` | The result wraps using two's complement. |

```neuro
func main() -> i32 {
    val x: u8 = 200u8
    val y: u8 = 100u8
    val z: u8 = x + y   // 300 does not fit in a u8: panics at -O0, wraps to 44 at -O2
    return z as i32
}
```

The debug trap turns a silent miscalculation into an immediate failure during development. The check covers `+`, `-`, `*`, unary `-`, and the one overflowing division, `MIN / -1` (and `MIN % -1`).

A shift overflows when its amount is outside `0..bits` of the shifted type, a negative signed amount included. `<<` and `.shr(n)` then panic with `shift overflow` at `-O0`. At `-O1` and above the amount is masked to `amount & (bits - 1)`, so `1 << 40` on an `i32` is `1 << 8`.

Unary negation is `0 - x`, so it overflows wherever that subtraction does: at a signed type's `MIN`, and at every nonzero value of an unsigned type.

```neuro
mut y: u8 = 1u8
val n = -y          // panics at -O0, wraps to 255 at -O2
```

A negative literal written for an unsigned type is rejected at compile time instead of wrapping. Use `wrapping_sub` when the wrap is what you mean:

```neuro
val x: u8 = 0u8.wrapping_sub(1u8)   // 255
```

A `const` initializer is folded by the compiler and never reaches either tier. An initializer whose arithmetic overflows its declared type is a compile error:

```neuro
const C: u8 = 200u8 + 100u8   // error: constant expression has no value: `+` overflows u8, whose values do not include the result
```

Write the value you mean, or widen the type. The bitwise operators `&`, `|`, `^` and `~` have no overflow rule and fold exactly as they evaluate at run time. A `<<` folds when its amount is inside `0..bits` and is a compile error otherwise. An explicit `as` still narrows: `300 as u8` is `44` in a `const` as it is in a function body.

### Integer Methods

When the overflow behavior matters, request it with a builtin method. These are compiler-known intrinsics, not user-defined `impl` methods. They dispatch on any integer receiver and take one argument of the receiver's type, except `.abs()`, which takes none.

| Method | Returns | Behavior |
|--------|---------|----------|
| `.wrapping_add(rhs)` / `.wrapping_sub(rhs)` / `.wrapping_mul(rhs)` | receiver type | Two's-complement wrap on overflow. Never traps, in any build. |
| `.saturating_add(rhs)` / `.saturating_sub(rhs)` / `.saturating_mul(rhs)` | receiver type | Clamps to the type's `MIN` / `MAX` instead of overflowing. |
| `.checked_add(rhs)` / `.checked_sub(rhs)` / `.checked_mul(rhs)` | `Option<T>` | `Option::Some(result)` when it fits, `Option::None` when it would overflow. |
| `.abs()` | receiver type | Signed integers only. `MIN.abs()` does not fit: a panic in a debug build, `MIN` in a release one. |
| `.shr(n)` | receiver type | Right shift by `n`. Arithmetic for signed types, logical for unsigned. Right shift is a method, not an operator (see [operators.md](operators.md)). |

```neuro
val a: u8 = 200
val b: u8 = 100
val wrapped: u8   = a.wrapping_add(b)     // 44   (200 + 100 = 300, wraps mod 256)
val saturated: u8 = a.saturating_add(b)   // 255  (clamps to MAX)
val floored: u8   = b.saturating_sub(a)   // 0    (unsigned underflow clamps to 0)
val shifted: u8   = a.shr(2)              // 50   (200 >> 2, logical)
```

`checked_*` is the reporting form. The overflow is not hidden, and the caller must unwrap the [`Option`](../../examples/types/option_result.nr) before using the value. `T` is the receiver's own type, so `200u8.checked_add(100u8)` is an `Option<u8>`.

```neuro
val a: u8 = 200
val b: u8 = 100
val checked: Option<u8> = a.checked_add(b)
val total: u8 = match checked {
    Option::Some(v) => v,
    Option::None => 0u8               // 300 does not fit in a u8
}
```

None of these methods trap.

### Floating-Point Types

| Type | Size | Precision | Range (approx) |
|------|------|-----------|----------------|
| `f16` | 16-bit | ~3 decimal digits | ±6.1e-5 to ±65504 |
| `bf16` | 16-bit | ~2 decimal digits | ±1.18e-38 to ±3.39e38 |
| `f32` | 32-bit | ~7 decimal digits | ±1.18e-38 to ±3.40e38 |
| `f64` | 64-bit | ~15 decimal digits | ±2.23e-308 to ±1.80e308 |

`f16` is the IEEE-754 half float. `bf16` is bfloat16, which trades mantissa bits for an `f32`-sized exponent range. Both are ordinary floats, computed through `f32` (see [Half-Precision Types](#half-precision-types-f16--bf16)).

**Default type**: a float literal is `f64` unless context says otherwise.

**Type suffixes**: a suffix appended directly to a float literal overrides contextual inference:

```neuro
val a = 1.5f32        // f32, no annotation needed
val b = 2.0f64        // f64
val c = 1e10f32       // exponent form with suffix
val d = 1.5e-5f64     // fractional + exponent with suffix
```

The valid suffixes are `f16`, `bf16`, `f32`, and `f64`. The suffix attaches directly to the literal with no whitespace between. A float literal that rounds to infinity in its type is a compile error: `1e400`, `1e39f32`, and `70000.0f16` are all refused.

#### Float Methods

| Method | Returns | Behavior |
|--------|---------|----------|
| `.is_nan()` | `bool` | `true` when the receiver is NaN, `false` for every other value including `Inf` and `-Inf`. Nullary. |
| `.exp()`, `.log()`, `.sqrt()`, `.tanh()`, `.abs()` | the receiver's type | The function of the receiver, IEEE 754 out of domain (`.log()` of `0.0` is `-inf`). Nullary. Float tensors have them too, see [tensors](tensors.md#elementwise-math). |
| `.pow(p)` | the receiver's type | The receiver raised to `p`, a value of the receiver's type. |
| `.to_checked::<T>()` | `Option<T>` | The receiver truncated toward zero as the integer type `T`, or `None` when the result does not fit `T` or the receiver is NaN. See [Float to integer](#float-to-integer). |

Every float type has these methods, `f16` and `bf16` included.

Floats follow IEEE 754, so every comparison against NaN is false, including `NaN == NaN` and `NaN != NaN`. The comparison operators cannot detect NaN, and `.is_nan()` is the test for it:

```neuro
val zero: f64 = 0.0
val nan: f64 = zero / zero    // 0.0 / 0.0 is NaN
val inf: f64 = 1.0 / zero     // division by zero is Inf, an ordinary ordered value

val a: bool = nan.is_nan()    // true
val b: bool = inf.is_nan()    // false: infinity is not NaN
val c: bool = nan == nan      // false: the equality operator cannot see it
```

### Half-Precision Types (`f16` / `bf16`)

`f16` and `bf16` are ordinary floats, the storage types of mixed-precision training. They support binding and `Copy`, `as` casts to and from every numeric type, arithmetic (`+ - * / %`, unary `-`, compound assignment), the six comparisons, the [float methods](#float-methods), and `.to_checked`. A literal takes its type from context (`val h: f16 = 0.5`, `h * 2.0`) or from a suffix (`1.5f16`, `0.02bf16`).

**Each operation is correctly rounded to the operand type.** `a + b` on two `bf16` values computes in `f32` and rounds the answer to `bf16` once. `f32` carries more than twice either format's significand bits, so that one rounding gives the IEEE 754 result for `+ - * /` and `.sqrt()` on every target. There is no excess precision: `a * b + c` rounds after `*` and again after `+`, at every optimization level.

```neuro
val one: bf16 = 1.0
val step: bf16 = 0.00390625                  // 2^-8, half the gap above 1.0
val stepwise = one + step + step             // 1.0: each addition is a tie, rounded to even
val carried = (one as f32 + step as f32 + step as f32) as bf16   // 1.0078125
```

There is no implicit widening. An `f16` meets only `f16`, so `h + 1.0f32` and `h + 1` are type errors. A program that wants `f32` accumulation writes `as f32`, as `carried` does. An interpolated hole prints a half value exactly as it prints an `f32`: `"{h}"` and `"{h as f32}"` give the same text.

As tensor element types they take the tensor operators and reductions. A reduction accumulates in `f32`, so a long `bf16` sum does not stall the way a 16-bit running total would. See [Tensors](tensors.md).

### Digit Separators

Underscores may be placed between digits of any numeric literal. They carry no value, and they work in every base, in floats, in exponents, and alongside type suffixes.

```neuro
val million = 1_000_000      // decimal grouping
val mask    = 0xFF_FF        // hex
val flags   = 0b1010_0011    // binary
val perms   = 0o7_5_5        // octal
val ratio   = 1_000.000_5    // float
val scaled  = 1_0e1_0        // exponent
val wide    = 1_000_000i64   // with a type suffix
```

A separator is only recognized between digits. A leading underscore (`_1000`) makes an identifier, not a number.

### Boolean Type

`bool` holds `true` or `false`. It supports `&&`, `||`, and `!`, and it is the result type of comparisons. `bool` casts to an integer type (`true as i64` is `1`), but an integer does not cast to `bool`.

### Character Type

The `char` type is a single **Unicode scalar value**, a 32-bit code point, not a byte.

| Property | Behavior |
|---|---|
| **Width** | 32-bit Unicode scalar value |
| **Literals** | `'a'`, `'\n'`, `'\t'`, `'\r'`, `'\\'`, `'\''`, `'\0'`, `'\xNN'`, `'\u{...}'` |
| **Copy** | Yes, binding a `char` copies it; the source remains valid |
| **Comparison** | Built-in `==`, `!=`, `<`, `>`, `<=`, `>=` (ordered by code point) |
| **Casts** | `as` to/from any integer type (`'A' as i32` is `65`, `97 as char` is `'a'`); **not** to/from `float` or `bool` |
| **Arithmetic** | None. `'a' + 1` is a compile error; cast to an integer and compute there |

An empty literal (`''`), a multi-character literal (`'ab'`), and an unterminated literal (`'a`) are lexer errors.

## Void Type

A function with no `->` clause returns `void`. A `void` function has no value, so binding its result is an error. The explicit `-> void` spelling is also accepted, and `return` may be bare in it.

```neuro
func log_line() {
    // No return type: the function returns void
}

func log_nothing() -> void {
    return
}
```

`main` is the exception: it must be declared `func main() -> i32`, because its return value is the exit status. See [Common Type Errors](#common-type-errors).

## Type Conversion

Explicit conversions use `as`. There are no implicit conversions.

| From | To | Result |
|------|----|--------|
| integer | integer | Keeps the low bits of the value: `300 as u8` is `44`, `-1i32 as u8` is `255`. |
| integer | float | The nearest float to the integer's value. |
| float | integer | Truncates toward zero, saturates to the target's range, and maps NaN to `0`. See [Float to integer](#float-to-integer). |
| `bool` | integer | `true` is `1`, `false` is `0`. |
| integer | `char` | The integer as a code point: `97 as char` is `'a'`. |
| `char` | integer | The code point: `'A' as i32` is `65`. |
| `f16` / `bf16` | any numeric type | Allowed. See [Half-Precision Types](#half-precision-types-f16--bf16). |
| integer | `bool` | Not allowed. Compare with `!= 0` instead. |
| float | `bool` or `char` | Not allowed. |
| `char` | float or `bool` | Not allowed. |

```neuro
func convert_types() -> i64 {
    val x: i32 = 42
    val y: i64 = x as i64      // widening
    val f: f64 = y as f64      // integer to float
    val pi: f64 = 3.14
    val trunc: i32 = pi as i32 // truncates to 3
    val flag: bool = true
    val num: i32 = flag as i32 // 1
    return y
}
```

### Float to integer

`as` from a float to an integer truncates toward zero. Every input has a defined result: a value the target cannot hold saturates to the target's minimum or maximum, and NaN becomes `0`. When a clamped answer would be wrong, use `.to_checked::<T>()`, which returns `Option<T>` and gives `None` in exactly the cases where `as` would saturate or map NaN:

```neuro
val reading: f64 = 300.7
val clamped: u8 = reading as u8                   // 255
val checked = reading.to_checked::<u8>() ?? 0u8   // None, so 0
val small = (2.9).to_checked::<i8>()              // Some(2)
```

`.to_checked` takes an `f16`, `bf16`, `f32` or `f64` value and exactly one integer type argument.

A cast whose operand is a compile-time constant (a float literal, a `const`, or arithmetic over those) that the target cannot hold is reported as the warning `float_cast_out_of_range`. The program still compiles, and the cast still saturates. Silence the warning with `@allow(float_cast_out_of_range)` on the enclosing function or method, or on the module-level `const` whose initializer holds the cast:

```neuro
@allow(float_cast_out_of_range)
const CEILING: i32 = 1e20 as i32
```

### No implicit conversions

Every binding, argument, return value, and operand has to match its type exactly. Passing an `i64` to an `i32` parameter, returning an `f64` from an `i32` function, and adding an `i32` to an `f64` are all type mismatches. The fix is an explicit `as`:

```neuro
func mixed(a: i32, b: f64) -> f64 {
    return (a as f64) + b
}
```

The messages are listed under [Common Type Errors](#common-type-errors).

## Arrays

A fixed-size array `[T; N]` holds exactly `N` values of element type `T`. `N` is part of the type, so `[i32; 3]` and `[i32; 4]` are distinct types. `N` is an integer literal, or, inside a generic definition, a `const` generic parameter (`[T; CAP]`), resolved to a concrete length by monomorphization.

```neuro
val a: [i32; 4] = [10, 20, 30, 40]   // explicit type
val b = [1, 2, 3]                    // inferred [i32; 3]

val first = a[0]                     // index read
mut c = [0, 0, 0]
c[1] = 42                            // element assignment (mut binding)
val n: u64 = a.len()                 // 4, a compile-time constant

for x in a {  }                      // iterate by value
for x in &a {  }                     // iterate over a borrow
```

- **Element type**: any sized type. An array of `Copy` elements is itself `Copy`. An array holding an owner (a `string`, a non-`Copy` struct) moves rather than copies, and reading one element out moves that element. See [Ownership of an element](ownership.md#ownership-of-elements).
- **Literals** must be homogeneous. The length is the element count, which must equal `N` when a `[T; N]` annotation is present.
- **Bounds**: an out-of-range index panics in every build, with `panic: array index out of bounds at <file>:<line>:<col>`. Unlike integer overflow, which wraps in release builds, an index past the end has no defined result to fall back on.
- **Iteration**: `for x in arr` and `for x in &arr` bind each element in order, and `for (i, x) in arr.enumerate()` also binds its `u64` position (see [Control Flow](control-flow.md)).

## Borrowed Slices (`&[T]` / `&mut [T]`)

`&[T]` is a non-owning `(ptr, len)` view over a contiguous run of `T`. It is the array and `Vec` counterpart of [`&string`](ownership.md#string-slices-string). A function that only reads or writes elements takes a slice, and does not care whether they come from a `[T; N]`, a `Vec<T>`, or part of either.

```neuro
func sum(xs: &[i32]) -> i32 {
    mut total = 0
    for x in xs { total = total + x }
    total
}

func double_each(xs: &mut [i32]) {
    mut i: u64 = 0
    while i < xs.len() {
        xs[i] = xs[i] * 2
        i = i + 1
    }
}

func main() -> i32 {
    mut fixed: [i32; 4] = [1, 2, 3, 4]
    mut grown: Vec<i32> = Vec::new()
    grown.push(9)
    double_each(&mut fixed)
    val a = sum(&fixed)                  // whole array
    val b = sum(fixed.slice(1..3))       // sub-range, zero copy
    val c = sum(&grown)                  // Vec, same signature
    return a + b + c                     // 20 + 10 + 9
}
```

- **`[T]` alone is unsized** and never appears outside a reference. The owned forms are `[T; N]` and `Vec<T>`.
- **Unsizing**: `&[T; N]`, `&Vec<T>`, and `&[T]` all satisfy a `&[T]` parameter, and the `&mut` forms satisfy a `&mut [T]` one. There is no `&T` to `&mut T` strengthening. A `&mut` binding passed to a `&` parameter is a shared reborrow for the call, and one call may not reborrow the same `&mut` twice unless both parameters are `&`.
- **`.slice(range)`** on an array, a `Vec<T>`, or another slice yields a `&[T]` view over the named sub-range, copying nothing. It accepts `a..b` and `a..=b`. An out-of-range or reversed range panics in every build, because the view outlives the check.
- **`.len()`** is O(1), read from the view's length: the borrowed run's length, not the container's.
- **Indexing** is bounds-checked as on the owning container. `xs[i] = v` requires a `&mut [T]`, and the write reaches the buffer, so the owner sees it.
- **Iteration**: a slice is a borrow, so `for x in xs` binds each element as a `&T`, or as a `&mut T` over a `&mut [T]`, which writes the element in place (`*x += 1`). `.enumerate()` works on a slice as it does on an array.
- **Borrow rules** are the ordinary ones. A live view counts as a shared borrow of the place it came from, so a `&mut` of that place is rejected while the view is alive. Returning a view of a function-local buffer is rejected as a dangling reference. See [Ownership and Borrowing](ownership.md).

## Tuples

A tuple `(T1, T2, ...)` is an anonymous, fixed-size, heterogeneous aggregate. Element types may differ, and the arity is part of the type, so `(i32, bool)` and `(i32, i32)` are distinct.

```neuro
val pair: (i32, i32) = (12, 30)     // explicit type
val mixed = (5, true, 'a')          // inferred (i32, bool, char)

val a = pair.0                      // index access by constant position
val b = pair.1

val (x, y) = pair                   // destructuring bind
val (_, keep, _) = mixed            // `_` discards an element
val ((p, q), r) = ((1, 2), 3)       // nested destructuring
```

- **Element type**: any sized type. A tuple is `Copy` exactly when every element is, and otherwise moves, on the same terms as an array element. See [Ownership of an element](ownership.md#ownership-of-elements).
- **Grouping vs. tuple**: a single parenthesized expression `(x)` is grouping. There is no one-element tuple, so `(x,)` is an error, and a tuple literal holds no elements or at least two.
- **Unit**: `()` is the tuple of no elements, as a type and as a value. It carries nothing, which is what a success payload with nothing to hand back wants: `func check(n: i32) -> Result<(), i32>` ends with `Ok(())`. `-> ()` means the same as leaving the return type out.
- **Index access**: `t.0`, `t.1`, and so on read by a constant index. An out-of-range index is a compile error, and indices chain (`t.0.1`). An element of a `mut` tuple is a place: `pair.0 = 7`, `pair.1 += 1`.
- **Destructuring**: `val (a, b) = t` binds each position. `_` is a wildcard, and patterns nest. Struct and array patterns work too. See [Variables](variables.md#destructuring).
- **Function boundaries**: tuples may be passed as parameters and returned, as in `func min_max(a: i32, b: i32) -> (i32, i32)`.

## Type Aliases

A `type` declaration introduces a **transparent** alias for an existing type. The alias and its target are interchangeable: no new type is created, so a value of either is the same type to the compiler. Contrast [`newtype`](#newtype-declarations), which is a distinct type.

```neuro
type Meters = f64
type Id = i64

struct Sensor {
    location: Meters        // same as `location: f64`
}

func to_id(raw: i32) -> Id {
    raw as Id               // same as `raw as i64`
}

func main() -> i32 {
    val s = Sensor { location: 10.0 }
    val tag: Id = to_id(7)
    return tag as i32       // 7
}
```

Aliases resolve in every type-annotation position: variable and `const` annotations, function parameters and return types, struct fields, and `as` cast targets. Alias chains collapse to their ultimate target:

```neuro
type A = B
type B = i32

func main() -> i32 {
    val x: A = 1            // x is i32
    return x
}
```

Aliases are resolved at parse time, so they cost nothing at run time and compile to the same code as the target type. An alias is not usable as a value-position constructor or path name (`MyAlias { ... }`).

**Rules and diagnostics**:

- A duplicate name is an error: `duplicate type alias 'C'`.
- An alias may not shadow a built-in type: `type alias 'i32' shadows a built-in type; choose a different name`.
- A cyclic chain is an error: `type alias 'A' is defined in terms of itself (cyclic alias)`.
- An unknown target is reported where the alias is used, against the resolved name: `unknown type name 'Nope'`.

## Newtype Declarations

A `newtype` creates a **distinct nominal type** that wraps an inner type. Unlike a `type` alias, a newtype and its inner type are different types, so the compiler will not mix them. This gives unit-of-measure and domain-identifier safety at no run-time cost. Structs and enums are also nominal, and they are covered on [Structs](structs.md).

```neuro
newtype Meters = i32
newtype Seconds = i32
newtype Celsius = f64
```

### Construction and Inner Access

Build a newtype value by calling the newtype name with one inner-typed argument. Read the wrapped value back with `.0`:

```neuro
newtype Meters = i32

func main() -> i32 {
    val d: Meters = Meters(30)     // construction
    val raw: i32 = d.0             // inner access
    return raw
}
```

`.0` also reads through a borrow, so `m.0` works on `m: &Meters`. It is read-only: assigning to it is an error even on a `mut` binding. Replace the whole value instead, with `m = Meters(5)`.

### Distinctness

```neuro
newtype Meters = i32
newtype Seconds = i32

val m: Meters = Meters(3)
val bad: i32 = m                       // error: type mismatch: expected i32, found Meters
val also_bad = Meters(1) + Seconds(2)  // error: cannot apply binary operator + to types Meters and Seconds
```

A newtype forwards `Copy` and `Clone` from its inner type. A `Copy`-inner newtype is itself `Copy`, and `newtype Name = string` moves like the `string` it wraps. It can be a `val` or `mut` binding, a parameter or return type, and a struct field.

### Methods and Trait Impls

A newtype takes `impl` blocks and trait impls exactly as a struct does, including the operator traits. That is how it gets arithmetic:

```neuro
newtype Meters = i32

impl Add for Meters {
    type Output = Meters
    func add(self, rhs: Meters) -> Meters { Meters(self.0 + rhs.0) }
}

impl Meters {
    func double(&self) -> Meters { Meters(self.0 * 2) }
}
```

Assigning to the inner value is an error. The message tells you to rebuild the whole value:

```text
error: cannot assign to the inner value of newtype 'Meters': `.0` reads it but is not a place; rebuild the whole value instead, e.g. `x = Meters(v)`
```

### Type Errors

| Error | Cause |
|---|---|
| `NewtypeAlreadyDefined` | A newtype reuses a builtin, struct, enum, or newtype name |
| `CyclicNewtype` | A newtype wraps itself directly or transitively |

## Tensor Types

`Tensor<T, [d0, d1, ...]>`, its shape system, indexing, and devices are on [Tensors](tensors.md).

## Standard Collections

Beyond `[T; N]`, the standard library provides three heap-backed collections. They are library types, but the language has no allocator, so the compiler knows all three by name and lowers their operations directly.

```neuro
mut counts: Vec<i32> = Vec::new()               // growable contiguous array
mut stock: HashMap<string, i32> = HashMap::new() // average-O(1) lookup
mut ranks: BTreeMap<i32, i32> = BTreeMap::new()  // key-ordered map
```

`Vec::new()` and its siblings carry no value to infer an element type from, so the binding must be annotated.

### Ownership

All three own a heap buffer, so none is `Copy`. Assignment and by-value passing **move** them, and the buffer is freed when the owner leaves scope. A method that mutates (`push`, `pop`, `insert`, `remove`, `clear`) needs a `mut` binding, as a `&mut self` method does.

```neuro
mut a: Vec<i32> = Vec::new()
val b: Vec<i32> = a       // moves; `a` is invalid from here
```

A collection owns what its slots hold, and an element may be any sized value: a scalar, a `string`, a struct, a tensor, or another collection. The rules are those for any other place (see [Ownership of an element](ownership.md#ownership-of-elements)):

- Insertion moves. `v.push(x)` and `m.insert(k, x)` take their arguments, so the bindings they came from are invalid afterward. To keep a value and also store it, `.clone()` it first.
- Indexing names a place. `v[i]` is the slot itself: `v[i] = x` writes it, `v[i].field` and `v[i].method()` work in place, and `&v[i]` and `&mut v[i]` borrow it. Using a non-`Copy` `v[i]` by value is an error, because a slot cannot be left empty.
- Lookup borrows. `get` lends `Option<&T>`, and the collection stays borrowed while the result lives. Maps also have `get_mut`, which lends `Option<&mut T>`. `pop`, `remove`, and a displacing `insert` hand the value over.
- A slot that is overwritten, removed, or cleared drops what it held. The collection drops its live slots when it is dropped.

```neuro
func main() -> i32 {
    val name: string = "ada" + "!"
    mut names: Vec<string> = Vec::new()
    names.push(name)                    // moves; `name` is invalid from here
    val kept = names[0].clone()         // an independent copy
    val first_len = names[0].len()      // a method on the slot, in place
    val taken = names.remove(0)         // takes the element out
    return first_len as i32             // 4
}
```

### `Vec<T>`

| Operation | Result | Notes |
| --- | --- | --- |
| `Vec::with_capacity(n)` | `Vec<T>` | Allocates room for `n` elements up front |
| `v.push(x)` | nothing | Appends, moving `x` in; grows the buffer as needed |
| `v.pop()` | `Option<T>` | Takes the last element out; `None` when empty |
| `v.get(i)` | `Option<&T>` | The checked borrow |
| `v.remove(i)` | `T` | Takes element `i` out and shifts the tail down |
| `v.swap_remove(i)` | `T` | Takes element `i` out and moves the last element into its slot |
| `v[i]` / `v[i] = x` | the slot / nothing | Panics out of range, in every build |
| `v.len()` | `u64` | Live element count |
| `v.clear()` | nothing | Drops every element, keeping the buffer |
| `for x in v` | nothing | Iterates the live elements in order, consuming `v` |
| `for x in &v` / `&mut v` | nothing | Iterates the elements as `&T` / `&mut T`, leaving `v` usable |

`for x in v` consumes `v` whatever its element type. Each pass owns the element it is handed, and leaving the loop early drops the ones it did not reach. `for x in &v` binds each element as a `&T`, read in place. `for x in &mut v` binds a `&mut T` that writes the slot. Either way `v` stays borrowed for the whole loop, so the body cannot push to it.

A `Vec` index is checked in every build, as an array's is. Because a `Vec`'s length is known only at run time, the optimizer can rarely prove the check redundant.

### `HashMap<K, V>` and `BTreeMap<K, V>`

Both share one surface:

| Operation | Result | Notes |
| --- | --- | --- |
| `m.insert(k, v)` | `Option<V>` | Moves both in. An existing key keeps its slot and hands back the value it held |
| `m.get(k)` | `Option<&V>` | `None` when absent |
| `m.get_mut(k)` | `Option<&mut V>` | Holds the map exclusively while the result lives |
| `m.contains_key(k)` | `bool` | |
| `m.remove(k)` | `Option<V>` | Takes the value out, dropping the key |
| `m.len()` | `u64` | Live entry count |
| `m.clear()` | nothing | |
| `m.keys()` | `Vec<K>` | A fresh `Vec`, so a map is iterated with `for k in m.keys()` |

`HashMap` has average-O(1) lookup. `BTreeMap` keeps its entries ordered by key, so `keys()` returns them **ascending**. Pick it when iteration order matters.

### Element and key types

- **Elements and values** may be any sized value except one that holds a reference or a function value.
- **Keys** also need equality, plus a hash (`HashMap`) or a total order (`BTreeMap`). The compiler supplies both for integer, `bool`, `char`, and `string` keys. A struct key needs `PartialEq` and `Hashable` (`HashMap`), or `PartialEq` and `Comparable` (`BTreeMap`), derived or written by hand. A type parameter is a key when its bounds name those traits and `Copy`, as in `func count<K: Hashable + PartialEq + Copy>`. See [Generics and Traits](generics-and-traits.md).
- **Float keys are rejected.** IEEE-754 comparison is a partial order, and NaN compares false against everything, itself included. A map keyed on a raw float could hold a key it can never find again. Use the prelude's validating wrappers, which reject NaN when they are built:

```neuro
mut scores: BTreeMap<OrderedF32, i32> = BTreeMap::new()
scores.insert(OrderedF32::new(0.75f32), 3)
```

`OrderedF32` and `OrderedF64` implement `Comparable`, not `Hashable`. Hashing a float has no representation-independent answer, so they are ordered-map keys only.

### `Hashable`

`Hashable` is a prelude trait with one method, `hash(&self) -> u64`. A struct derives it (see [Structs](structs.md)) or implements it by hand:

```neuro
struct Point { x: i32, y: i32 }

impl Hashable for Point {
    func hash(&self) -> u64 {
        (self.x * 31 + self.y) as u64
    }
}
```

The one requirement is that equal keys hash equally. The integers, `bool`, `char`, and `string` already implement it. `.hash()` on one of them, or on a struct that derives it, returns the hash the map uses for that key.

### Current limits

- An element may not hold a reference or a function value.
- `keys()` builds a `Vec` holding a copy of every key rather than an iterator over borrows, and there is no `values()`.
- `Vec<T>` does not go through the [`Iterator` protocol](control-flow.md#the-iteration-protocol). `for x in v` lowers to a counted loop, as `for x in arr` does, and so does `for (i, x) in v.enumerate()`. The protocol is what a user-defined type implements to stand in a `for` head.

## Type Annotations and Inference

### Variable Declarations

Annotations are optional when the type can be inferred from context:

```neuro
val x: i32 = 42              // explicit type annotation
val pi: f64 = 3.14159        // explicit type annotation
val n = 100                  // inferred i32 (default for integer literals)
val e = 2.71828              // inferred f64 (default for float literals)
```

### Function Parameters and Return Types

Parameters must have explicit type annotations. A return type must be written too, unless the function returns `void`:

```neuro
func add(a: i32, b: i32) -> i32 {
    return a + b
}
```

Generic parameters, trait bounds, and `where` clauses are covered on [Generics and Traits](generics-and-traits.md).

## Common Type Errors

The messages below are the compiler's text. The output puts the file and position above each one. A type mismatch looks like this:

```text
error: type mismatch: expected i32, found bool
 --> program.nr:2:5
  |
2 |     val x: i32 = true
  |     ^^^^^^^^^^^^^^^^^
```

| Message | Cause |
|---|---|
| `type mismatch: expected i32, found bool` | A binding, argument, or operand has a different type from the one expected. Argument mismatches use the same text. |
| `return type mismatch: expected i32, found bool` | A `return` value does not match the declared return type. |
| `integer literal 300 out of range for type i8` | A literal, with or without a suffix, does not fit its type. An unannotated literal past `i32` gets the same message. |
| ``literal -1 is negative and u8 is unsigned: use a signed type, or `0u8.wrapping_sub(1u8)` if the two's-complement wrap was intended`` | A negative literal for an unsigned type. |
| `float literal out of range for type f32: its value rounds to infinity` | A float literal that rounds to infinity in its type. |
| ``constant expression has no value: `+` overflows u8, whose values do not include the result`` | A `const` initializer whose arithmetic overflows its type. |
| `cannot apply binary operator + to types char and i32` | Arithmetic on a `char`. Cast it to an integer first. |
| `type mismatch: expected bool, found i32` | `1 as bool`. An integer does not cast to `bool`. |
| `array literal has 2 elements but type annotation expects 3` | An array literal whose length differs from its `[T; N]` annotation. |
| `tuple index 2 is out of range: the tuple has 2 elements` | A tuple index past the last element. |
| ``a tuple has zero or at least two elements: `(x,)` is not a tuple; write one value as itself`` | A one-element tuple literal. |
| ``` `[i32]` is unsized and must appear behind a reference: write `&[i32]` or `&mut [i32]` ``` | A parameter typed as a bare slice. |
| ``cannot infer the element type of `Vec::new()`; annotate the binding, e.g. `val v: Vec<...> = Vec::new()` `` | An unannotated empty collection. |
| ``f64 is not a valid `HashMap` key: floating-point values have no total order (NaN compares false against everything); wrap the key in the standard `OrderedF32` / `OrderedF64` struct, which rejects NaN`` | A float used as a map key. |
| ``` `main` must be declared `func main() -> i32`: it returns the exit status as `i32`, found `void` ``` | A `main` with any other return type. |

The three runtime failures are panics, not compile errors. Each names the location where it happened:

```text
panic: integer overflow at program.nr:4:17
panic: shift overflow at program.nr:2:15
panic: array index out of bounds at program.nr:4:12
```

## See also

- [Strings](strings.md): `string` and the growable `StringBuilder`
- [Tensors](tensors.md): `Tensor<T, [dims]>` and its shape system
- [Structs](structs.md): structs, enums, `impl` blocks, and derives
- [Ownership and Borrowing](ownership.md): moves, `&T` and `&mut T`, lifetimes, and `Drop`
- [Generics and Traits](generics-and-traits.md): type parameters, bounds, and dispatch
- [Variables](variables.md): bindings and destructuring
- [Operators](operators.md): type requirements for operators
