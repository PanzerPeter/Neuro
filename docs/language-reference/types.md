# Type System

Neuro is a statically typed language with explicit type annotations and planned type inference.

## Current Status

- Implemented: primitive types (integers, floats, booleans, `char`)
- Implemented: half-precision scalars (`f16`, `bf16`) with a narrow storage/cast/compare contract
- Implemented: extended integer types (`i8`-`i64`, `u8`-`u64`)
- Implemented: function types
- Implemented: void type
- Implemented: contextual inference for numeric literals
- Implemented: string type
- Implemented: structs (definition, instantiation, field access, field mutation)
- Implemented: fixed-size arrays `[T; N]` of `Copy` elements
- Implemented: tuples `(T1, T2, ...)` of `Copy` elements, with destructuring
- Planned (1F): generics
- Planned (1F): traits

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

#### Integer Methods

When the overflow behavior matters, request it explicitly with a builtin intrinsic method. These dispatch on any integer receiver, take one same-typed argument, and return the receiver's type. They are compiler-known intrinsics, not user-defined `impl` methods.

| Method | Behavior |
|--------|----------|
| `.wrapping_add(rhs)` / `.wrapping_sub(rhs)` / `.wrapping_mul(rhs)` | Two's-complement wrap on overflow. Never traps, regardless of build profile. |
| `.saturating_add(rhs)` / `.saturating_sub(rhs)` / `.saturating_mul(rhs)` | Clamp to the type's `MIN` / `MAX` instead of overflowing. |
| `.shr(n)` | Right shift by `n`. Arithmetic (sign-preserving) for signed types, logical for unsigned. Right shift is a method rather than an operator — see [operators.md](operators.md). |

```neuro
val a: u8 = 200
val b: u8 = 100
val wrapped: u8   = a.wrapping_add(b)     // 44   (200 + 100 = 300, wraps mod 256)
val saturated: u8 = a.saturating_add(b)   // 255  (clamps to u8::MAX)
val floored: u8   = b.saturating_sub(a)   // 0    (unsigned underflow clamps to 0)
val shifted: u8   = a.shr(2)              // 50   (200 >> 2, logical)
```

`checked_*` (which returns `Option<T>` on overflow) is deferred until `Option` lands in 1G.

### Floating-Point Types

| Type | Size | Precision | Range (approx) |
|------|------|-----------|----------------|
| `f16` | 16-bit | ~3 decimal digits | ±6.1e-5 to ±65504 |
| `bf16` | 16-bit | ~2 decimal digits | ±1.18e-38 to ±3.39e38 |
| `f32` | 32-bit | ~7 decimal digits | ±1.18e-38 to ±3.40e38 |
| `f64` | 64-bit | ~15 decimal digits | ±2.23e-308 to ±1.80e308 |

`f16` is the IEEE-754 half float; `bf16` is bfloat16, which trades mantissa bits for an `f32`-sized exponent range. Both are full scalar primitives with a deliberately **narrow contract** — see [Half-Precision Types](#half-precision-types-f16--bf16) below.

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

Valid suffixes: `f16`, `bf16`, `f32`, `f64`. The suffix attaches directly to the literal — no whitespace is permitted between the digits and the suffix. The exponent form (`1e10f32`) and the fractional form (`1.5f32`) both accept a suffix.

### Half-Precision Types (`f16` / `bf16`)

Modern AI relies on half-precision for mixed-precision training, so `f16` and `bf16` are first-class scalar primitives. To avoid the cross-hardware inconsistency of half-precision ALUs, they carry a **narrow scalar contract**:

| Operation | Supported? |
|-----------|------------|
| Binding, move/copy (`Copy`) | ✅ |
| Equality (`==`, `!=`) | ✅ |
| `as`-cast to/from any numeric type, and to/from each other | ✅ |
| Suffixed literals (`1.5f16`, `0.02bf16`) | ✅ |
| Arithmetic (`+`, `-`, `*`, `/`, `%`) | ❌ compile error |
| Ordering (`<`, `>`, `<=`, `>=`) | ❌ compile error |

Half-precision literals **must** carry their suffix — there is no contextual default, so `val x: f16 = 1.5` is an error; write `1.5f16`.

Scalar arithmetic is intentionally undefined: half-precision math is not portably specified across hardware. Compute in `f32` and cast back:

```neuro
func main() -> i32 {
    val a: bf16 = 10.0bf16
    val b: bf16 = 4.0bf16

    // val bad = a + b            // compile error: arithmetic not defined on bf16
    val sum: bf16 = (a as f32 + b as f32) as bf16   // 14.0

    val h: f16 = 1.5f16
    val same: bool = h == 1.5f16  // equality is allowed
    return sum as i32             // 14
}
```

As **tensor element types** (`Tensor<bf16, [...]>`, Phase 2) the restriction lifts entirely: elementwise math, matmul, and reductions lower through MLIR to the accelerator's native half-precision units. The split keeps half-precision where it pays off — bulk tensor compute — without committing the scalar layer to non-portable semantics.

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

### Character Type

The `char` type is a single **Unicode scalar value** (a 32-bit code point), not a byte.
Char literals are written with single quotes and support escape sequences:

```neuro
func demo_chars() -> i32 {
    val letter: char = 'A'
    val newline: char = '\n'
    val emoji: char = '\u{1F44D}'   // thumbs-up, U+1F44D

    // char is Copy, so the source stays valid after a bind.
    val also = letter

    // Built-in total order: all six comparison operators work directly.
    if letter < 'Z' && also == letter {
        return letter as i32        // as-cast char -> integer (65)
    }
    return 0
}
```

| Property | Behavior |
|---|---|
| **Width** | 32-bit Unicode scalar value |
| **Literals** | `'a'`, `'\n'`, `'\t'`, `'\r'`, `'\\'`, `'\''`, `'\0'`, `'\xNN'`, `'\u{...}'` |
| **Copy** | Yes — binding a `char` copies it; the source remains valid |
| **Comparison** | Built-in `==`, `!=`, `<`, `>`, `<=`, `>=` (ordered by code point) |
| **Casts** | `as` to/from any integer type (`'A' as i32`, `97 as char`); **not** to/from `float`/`bool` |
| **Arithmetic** | None — `'a' + 1` is a compile error; cast to an integer and compute there |

An empty literal (`''`), a multi-character literal (`'ab'`), and an unterminated literal
(`'a`) are all lexer errors.

## String Type

The `string` type is an immutable, UTF-8 encoded fat pointer `{ ptr, i64 }` — a pointer to
the bytes plus a stored byte length. Equality (`==`, `!=`) compares byte content; the `+`
operator concatenates two strings into a new owned `string` (§2.7).

### Storage and the `len` Guarantee

String **literals** live in read-only program memory (`.rodata`) for the lifetime of the
program; they are **not** heap-allocated, so a program that only reads literals never leaks.
**Concatenation** (`a + b`) is the first runtime heap-backed string: it `malloc`s a fresh buffer
and copies both operands' bytes in, yielding a new owned `string`. Both literal and heap-backed
forms share the same `{ ptr, i64 }` ABI, so consumers cannot tell them apart. Until `Drop` /
deterministic destruction lands (1C), concatenated buffers leak — see the alpha memory
warning in the README. (Growable builders — `String::new` / `.push_str` — also await that work.)

The pointer addresses a NUL-terminated byte sequence so it doubles as a valid C string for
future FFI, but the stored `len` field **excludes** that trailing NUL. `len` is the
**authoritative** length: it is the exact UTF-8 byte count of the content. Consumers must use
`len` and **must not** scan for a NUL terminator, because interior NUL bytes are legal content —
`"a\0b".len()` is `3`, not `1`.

### String Methods

Builtin intrinsic methods dispatch on a `string` receiver via the usual `receiver.method()`
syntax:

```neuro
val s: string = "hello, world"
val n: u64 = s.len()    // 12 — O(1) read of the stored byte length
val copy: string = s.clone()   // a fresh string equal to s
val hello: &string = s.slice(0..5)    // "hello" — borrowed, zero copy
val world: &string = s.slice(7..=11)  // "world" — inclusive upper bound
```

**`.len() -> u64`** — returns the number of UTF-8 bytes, read directly from the fat pointer
in O(1) with no scan. The length **excludes** the null terminator. Because the index is a
byte count, a multi-byte code point contributes more than one to the length.

**`.clone() -> string`** — returns a fresh `string` equal to its receiver. It is the
canonical explicit deep copy for non-`Copy` owned types and, now that move-by-default has
landed (1C, §2.2 — see [variables](variables.md#move-semantics-ownership)), the way to
keep using a value after it would otherwise be moved. Today strings
are immutable and `.rodata`-backed (no heap string type exists yet), so the clone copies the
`(ptr, len)` fat pointer — observationally a deep copy because the pointee bytes are
immutable and shared safely. `.clone()` takes no arguments and returns a `string`, so it
chains with other builtin methods (`"hi".clone().len()`). `Copy` scalar types
(`i8`..`u64`, `f32`/`f64`, `bool`) do not provide `.clone()`: assignment already duplicates
them.

**`.slice(range) -> &string`** — returns a borrowed `&string` view into the receiver's UTF-8
data, with no allocation: since strings are immutable, a sub-range is just a `(ptr + start,
len)` fat pointer (the analogue of Rust's `&str`). The range is exclusive (`s.slice(a..b)`)
or inclusive (`s.slice(a..=b)`). **Indices are byte offsets**, not character offsets. The
slice is itself a `&string`, so it chains (`s.slice(0..5).len()`) and compares byte-wise
(`s.slice(0..5) == "hello"`). Two boundary rules are enforced at runtime in **both** debug and
release builds and **panic** (abort, no unwinding — see [control flow](control-flow.md)) on
violation:

- **Bounds:** the range must satisfy `0 <= start <= end <= len`. An out-of-bounds or reversed
  range panics with `string slice out of bounds`.
- **Code-point alignment:** each endpoint must fall on a UTF-8 code-point boundary. A range
  that splits a multi-byte code point panics with `string slice splits a UTF-8 code point`.

A range expression `a..b` / `a..=b` is valid **only** as a `.slice` argument; used anywhere
else it is a compile error.

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

### Copy and Clone (`@derive`)

By default a struct is **move-by-default**, just like `string`: binding, assigning,
returning, or passing it by value moves the source, and reading the source afterward is a
`use of moved value` error (§2.2). A struct opts out of moving by deriving `Copy`:

```neuro
@derive(Copy, Clone)
struct Point { x: i32, y: i32 }

val a = Point { x: 3, y: 4 }
val b = a          // a is COPIED, not moved
val s = a.x + b.y  // a is still valid here
```

Rules (§2.3):

- A struct may derive `Copy` only when **every field is `Copy`**. Primitive scalars
  (`i8`–`u64`, `f32`, `f64`, `bool`) are `Copy`; `string` is not; a struct field is `Copy`
  only when its type also derives `Copy`. Violating this is a `CopyDeriveNonCopyField` error.
- `Copy` implies `Clone`.
- `@derive(Clone)` (or `Copy`) enables `struct.clone()` — an explicit deep copy that returns a
  fresh value without moving the receiver. A user-defined `clone` method in an `impl` block
  shadows the builtin.
- Unknown derive arguments (e.g. `@derive(Debug)`) are accepted and ignored for now.

```neuro
@derive(Clone)
struct Vec2 { x: f64, y: f64 }

val v = Vec2 { x: 1.0, y: 2.0 }
val w = v.clone()  // independent copy; v stays usable
```

### Type Errors

| Error | Cause |
|---|---|
| `MissingStructField` | Struct literal omits a declared field |
| `UnknownField` | Struct literal or access uses a field that doesn't exist |
| `AssignToImmutableField` | Field assignment on a `val` binding |
| `StructAlreadyDefined` | Two `struct` declarations share the same name |
| `UnknownStruct` | Struct literal references an undeclared struct name |
| `CopyDeriveNonCopyField` | `@derive(Copy)` on a struct with a non-`Copy` field |

## Enum Types (§3.5)

Enums are user-defined types that hold exactly one of several named **variants**. A variant may be a bare tag, carry a positional **tuple** payload, or carry **named fields** — all three may appear in one enum. Like structs, enums use nominal typing.

### Definition

```neuro
// Bare variants
enum Color {
    Red,
    Green,
    Blue
}

// Mixed variant shapes
enum Shape {
    Circle { radius: f64 },              // named-field variant
    Rectangle { width: f64, height: f64 },
    Triangle { base: f64, height: f64 }
}

enum Message {
    Quit,                                // unit variant
    Move(i32, i32),                      // tuple variant
    Write(bool)
}
```

### Construction

Each variant shape has its own construction syntax, all prefixed with the enum name:

```neuro
val c = Color::Red                       // unit variant
val m = Message::Move(1, 2)              // tuple variant
val s = Shape::Circle { radius: 5.0 }    // struct variant
```

An enum value can be bound to a `val`/`mut`, passed to and returned from functions, and stored in a struct field. Enums are **`Copy`** (their payloads are scalar `Copy` primitives — see below), so binding or passing one duplicates it rather than moving it.

### Memory Layout

An enum is a tagged union: a discriminant identifying the active variant, plus storage for the widest variant's payload. Two enums with the same variant names but declared separately are distinct types.

### Phase 1E Limitations

- **Payloads are scalar `Copy` primitives only** — integers, floats, `bool`, `char`. A payload of `string`, a struct, an array, a tuple, or a reference is rejected (`UnsupportedEnumPayload`); broader payloads arrive with pattern matching and heap support.
- **Non-generic** — generic enums such as `Option<T>` arrive with the generics system (1F).
- **No deconstruction yet** — reading a variant's tag or payload needs `match` (the next 1E item, §3.6). Until then an enum value is constructed and carried, not inspected.

### Type Errors

| Error | Cause |
|---|---|
| `EnumAlreadyDefined` | Two `enum` (or an `enum` and a `struct`) share a name |
| `UnknownEnumVariant` | Construction names a variant the enum does not declare |
| `EnumVariantFormMismatch` | A variant built with the wrong syntax (e.g. a struct variant called like a function) |
| `EnumVariantArityMismatch` | A tuple variant built with the wrong number of arguments |
| `UnknownEnumField` / `MissingEnumField` / `DuplicateEnumField` | Struct-variant field set is wrong |
| `UnsupportedEnumPayload` | A variant payload is not a scalar `Copy` primitive |

## Newtype Declarations (§3.15)

A `newtype` creates a **distinct nominal type** that wraps an inner type. Unlike a `type` alias — which is transparent, so the alias and its target are interchangeable — a newtype and its inner type are *different types*. This buys unit-of-measure and domain-identifier safety at zero runtime cost.

```neuro
newtype Meters = i32
newtype Seconds = i32
newtype Celsius = f64
```

### Construction and Inner Access

Build a newtype value by calling the newtype name with a single inner-typed argument, and read the wrapped value back with `.0`:

```neuro
val d: Meters = Meters(30)     // construction
val raw: i32 = d.0             // inner access
```

### Distinctness

Because a newtype is a separate type, its values are not interchangeable with the inner type or with another newtype over the same inner type:

```neuro
val m: Meters = Meters(3)
val bad: i32 = m               // ERROR: expected i32, found Meters
val also_bad = Meters(1) + Seconds(2)  // ERROR: arithmetic is not defined on newtypes
```

A newtype forwards `Copy`/`Clone` from its inner type, so a `Copy`-inner newtype is itself `Copy`. It can be a `val`/`mut` binding, a function parameter or return type, and a struct field.

### Phase 1E Limitations

- **Inner type must be `Copy`** — integers, floats, `bool`, `char`, and other `Copy` aggregates. A non-Copy inner such as `string` is rejected (`NewtypeInnerNotCopy`); non-Copy wrappers arrive with broader move/heap support.
- **No inherent methods or operator traits yet** — arithmetic and other operators on a newtype await the trait system (1F). Use `.0` to compute on the inner value.

### Type Errors

| Error | Cause |
|---|---|
| `NewtypeAlreadyDefined` | A newtype reuses a builtin, struct, enum, or newtype name |
| `NewtypeInnerNotCopy` | The wrapped inner type is not `Copy` |
| `CyclicNewtype` | A newtype wraps itself directly or transitively |

## References — Immutable Borrows (`&T`)

An **immutable borrow** `&T` is a non-owning reference to a value (§2.4). It lets a
function read a value without taking ownership, so the caller keeps using its binding
afterward. The borrow expression `&x` takes a reference to the place `x`.

```neuro
func describe(s: &string) -> u64 {
    s.len()                 // method call auto-derefs through the borrow
}

func main() -> i32 {
    val msg: string = "Neuro"
    val n: u64 = describe(&msg)   // borrow — msg is NOT moved
    val again: u64 = msg.len()    // still valid: borrowing never consumes
    return 0
}
```

**Rules:**

- A reference type `&T` may appear on parameters, return types, and local bindings.
- Borrowing **does not move** the borrowed value — that is the whole point of a reference.
  A non-`Copy` value such as `string` stays usable after being borrowed.
- `&T` is itself `Copy`: passing or re-borrowing a reference duplicates the pointer.
- Method and field access **auto-deref** through a borrow: `r.len()` / `r.clone()` on a
  `&string`, and `r.field` / `r.method()` on a `&Struct`, behave as if applied to the
  referent.
- Only a **place** (a `val`/`mut`/parameter binding) can be borrowed. Borrowing a
  temporary (a literal or a call result) or a `const` (an inlined value, not a memory
  location — §1.3) is a `CannotBorrowValue` error.

```neuro
struct Point { x: i64, y: i64 }
impl Point {
    func sum(&self) -> i64 { self.x + self.y }
}

func read_sum(p: &Point) -> i64 { p.sum() }   // borrow a struct, call through it
```

> **Not yet lifetime-verified:** a returned `&T` is not yet checked against the lifetime of
> the value it points into; that check lands with lifetime inference. Integer intrinsics
> (`r.wrapping_add(..)`) still require a value receiver — read through `*r` first.

### References — Mutable Borrows (`&mut T`)

A **mutable borrow** `&mut T` is a non-owning reference that grants **write** access to a
value (§2.5). The borrow expression `&mut x` requires `x` to be a `mut` binding — you
cannot acquire write access through a reference to a value you may not write directly.

Values are read and written through the prefix **dereference operator** `*`:

```neuro
func increment(n: &mut i32) {
    *n = *n + 1          // read with *n, write with *n = ...
}

func main() -> i32 {
    mut counter: i32 = 40
    increment(&mut counter)       // mutate in place — counter is borrowed, not moved
    increment(&mut counter)
    return counter                // 42
}
```

**Rules:**

- `&mut x` requires a `mut` binding; mutably borrowing a `val` is a compile error
  (`cannot mutably borrow`).
- `*r` reads the referent; dereferencing a non-reference is an error (`cannot dereference`).
- `*r = value` writes through the reference and requires `r: &mut T`; writing through an
  immutable `&T` is an error (`cannot assign through an immutable reference`).
- `&mut T` and `&T` are **distinct types** — there is no implicit `&mut T → &T` coercion
  (explicit over implicit).

```neuro
func main() -> i32 {
    mut x: i32 = 7
    val r: &mut i32 = &mut x
    *r = 35
    return *r                     // 35
}
```

### Borrow Exclusivity (`&` / `&mut` aliasing rules)

The borrow checker enforces two coexistence rules at compile time (§2.4, §2.5):

- **Any number of shared `&T` borrows** of a place may be live at the same time.
- **A `&mut T` borrow is exclusive**: while it is live, no other borrow of that place —
  shared or mutable — may exist.

A borrow's region is **lexical**. A borrow held by a binding (`val r = &x`) lives until that
binding leaves scope; a borrow passed to a function, used in a condition, or returned ends
with the statement that took it. So sequential borrows in separate statements never conflict,
and a borrow taken inside a block is released at the block's closing brace.

```neuro
func main() -> i32 {
    mut x: i32 = 5
    val a: &i32 = &x
    val b: &i32 = &x          // ok — shared borrows coexist
    val c: &mut i32 = &mut x  // ERROR: cannot borrow 'x' as mutable — it is already borrowed
    return 0
}
```

```neuro
func inc(n: &mut i32) { *n = *n + 1 }

func main() -> i32 {
    mut x: i32 = 0
    inc(&mut x)               // the &mut ends with this call
    inc(&mut x)               // ok — the previous borrow is no longer live
    return x                  // 2
}
```

The diagnostics are `cannot borrow '<name>' as mutable` (a `&mut` while any borrow is live) and
`cannot borrow '<name>' as immutable` (a `&` while a `&mut` is live).

> **Deferred:** this is a lexical check, not non-lexical liveness (NLL). Reading or moving a
> value while it is borrowed lands with full **lifetime inference**, which extends the same
> borrow-region analysis.

### Lifetimes — Returned References (§2.6)

Lifetimes are **inferred** — there is no annotation syntax yet. The elision rules match Rust: a
single input reference lifetime is applied to the outputs, and the `&self` lifetime is applied to
a method's outputs. In practice this means a function or method that **returns a reference** may
borrow one of its reference parameters (or, in a method, `&self`); the returned borrow then lives
as long as the caller's borrow.

```neuro
func first(a: &i32, b: &i32) -> &i32 {
    a                       // ok — the returned borrow outlives the call
}
```

The borrow checker rejects returning a reference to a value that dies when the function returns —
a body-local or a by-value parameter — because the reference would dangle:

```neuro
func dangle() -> &i32 {
    val local: i32 = 5
    return &local           // ERROR: cannot return a reference to 'local' — it is local to
                            //        this function and does not outlive the call
}
```

The check follows a returned reference through a local reference binding (`val r = &local; r` is
also rejected) and into the arms of a returned `if`/`else`. The diagnostic is
`cannot return a reference to '<name>'`.

> **Deferred:** this is elision only. Explicit lifetime annotations (`func longest<'a>(a: &'a string,
> b: &'a string) -> &'a string`) need a generic-parameter parse surface and land with **generics**
> (1F); until then an ambiguous multi-reference signature is accepted as long as the returned
> borrow targets a parameter.

### String Slices (`&string`)

`&string` is the **borrowed string slice** (§2.7): a non-owning `(ptr, len)` view into
UTF-8 data. There is no separate slice type — `&string` is both "a borrow of an owned
`string`" and "a string slice," the analogue of Rust's `&str`.

A slice is read-only, so its fundamental operation is **equality**. The operators `==`
and `!=` compare the underlying UTF-8 bytes for any combination of an owned `string` and a
`&string` slice; a borrowed operand is auto-dereferenced to its fat pointer before the byte
compare, so it costs no copy.

```neuro
func slices_equal(a: &string, b: &string) -> bool {
    a == b                       // two borrowed slices
}

func main() -> i32 {
    val lang: string = "Neuro"
    val same: string = "Neuro"
    val eq: bool = slices_equal(&lang, &same)   // true
    val lit: bool = (&lang == "Neuro")          // true — slice vs owned literal
    if eq && lit { return 0 }
    return 1
}
```

Comparing through borrows never moves: `lang` stays usable after each `&lang`. Reference
peeling for equality is **string-only**, so comparing a non-string reference to its value
(`&n == n` on an `i32`) or mixing types (`i32 == &string`) is still a type error — reading
other `&T` through `==` needs the `*` deref operator (§2.5).

> **Not yet available:** the slicing methods `.slice(range)` / `.char_slice(range)` that
> produce a `&string` view into the interior of a string arrive in a later phase; today a
> `&string` is obtained only by borrowing an owned `string` with `&`.

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

### Phase 1 — Core Language

**Landed (✅):**
- Primitive types (i8-i64, u8-u64, f32, f64, bool, char, f16/bf16)
- String type with fat pointer ABI (`{ ptr, i64 }`)
- Explicit type annotations + contextual numeric inference with range validation
- Explicit type conversions via `as`
- Function types, strict type checking, type-mismatch error reporting
- Structs, methods, fixed-size arrays `[T; N]`, tuples + destructuring, type aliases
- Enums with associated data `enum E { A, B(T), C { f: T } }` (§3.5)

**In progress / planned (still Phase 1):**
- Pattern matching, newtypes (1E)
- Generics + monomorphization, traits, operator traits, static/dynamic dispatch, closures (1F)
- `Option` / `Result`, collections, modules, prelude (1G)

### Phase 2 — Tensors (Planned)

- Static tensor types: `Tensor<f32, [3, 3]>`
- Compile-time shape checking
- Broadcasting rules
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

## Type Aliases

A `type` declaration introduces a **transparent** alias for an existing type. The
alias and its target are fully interchangeable — no new nominal type is created,
so a value of the alias type and a value of the target type are the same type to
the compiler (contrast with `newtype`, §3.15, which is a distinct type).

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

Aliases resolve in every type-annotation position: variable and `const`
annotations, function parameters and return types, struct fields, and `as` cast
targets. Alias chains collapse to their ultimate target:

```neuro
type A = B
type B = i32
val x: A = 1                // x : i32
```

**Rules and diagnostics:**

- A duplicate alias name is a compile error.
- An alias may not shadow a built-in type name (`i32`, `f64`, `string`, …).
- A cyclic alias chain (`type A = B`; `type B = A`) is a compile error.
- An unknown target type is reported where the alias is *used*, against the
  resolved name.

Aliases are resolved at parse time, so they carry zero runtime cost and produce
exactly the same code as writing the target type directly. Using an alias as a
value-position constructor or path name (e.g. `MyAlias { ... }`) is not part of
this feature.

## Arrays (§3.1)

A fixed-size array `[T; N]` holds exactly `N` values of element type `T`, with
`N` fixed at compile time and part of the type — `[i32; 3]` and `[i32; 4]` are
distinct types.

```neuro
val a: [i32; 4] = [10, 20, 30, 40]   // explicit type
val b = [1, 2, 3]                    // inferred [i32; 3]

val first = a[0]                     // index read
mut c = [0, 0, 0]
c[1] = 42                            // element assignment (mut binding)
val n = a.len()                      // 4, as u64 (compile-time constant)

for x in a {  }                      // iterate by value
for x in &a {  }                     // iterate over a borrow
```

- **Element type**: currently restricted to `Copy` scalar primitives (the
  integer types, `f16`/`bf16`/`f32`/`f64`, `bool`, `char`). An array of `Copy`
  elements is itself `Copy`. Arrays of non-`Copy` elements (strings, structs)
  are not yet supported.
- **Literals** must be homogeneous; the length is the element count and, when a
  `[T; N]` annotation is present, must equal `N`.
- **Bounds**: an out-of-range index panics with a located diagnostic in debug
  builds (`-O0`); release builds omit the check (matching the integer-overflow
  policy).
- **Iteration**: `for x in arr` / `for x in &arr` bind each element in order;
  the indexed form `for (i, x) in arr.enumerate()` arrives with tuples (§3.2).

## Tuples (§3.2)

A tuple `(T1, T2, ...)` is an anonymous, fixed-size, **heterogeneous** aggregate.
Element types may differ; the arity is part of the type, so `(i32, bool)` and
`(i32, i32)` are distinct.

```neuro
val pair: (i32, i32) = (12, 30)     // explicit type
val mixed = (5, true, 'a')          // inferred (i32, bool, char)

val a = pair.0                      // index access by constant position
val b = pair.1

val (x, y) = pair                   // destructuring bind
val (_, keep, _) = mixed            // `_` discards an element
val ((p, q), r) = ((1, 2), 3)       // nested destructuring
```

- **Element type**: currently restricted to `Copy` types, so a tuple is itself
  `Copy`. Tuples holding a `string` or another non-`Copy` value are not yet
  supported (the same restriction as array elements).
- **Grouping vs. tuple**: a single parenthesized expression `(x)` is grouping,
  not a one-element tuple. A tuple literal needs at least two elements.
- **Index access**: `t.0`, `t.1`, … read by a constant index; an out-of-range
  index is a compile error. (Because `t.0.1` lexes as the float `0.1`, write a
  nested access as `(t.0).1`.)
- **Destructuring**: `val (a, b) = t` binds each position; `_` is a wildcard, and
  patterns nest. It is a binding form, not a new value — it desugars to ordinary
  bindings.
- **Function boundaries**: tuples may be passed as parameters and returned, e.g.
  `func min_max(a: i32, b: i32) -> (i32, i32)`.

Struct and array destructuring patterns are also supported: `val Point { x, y } = p`
binds each named field, and `val [first, second, ..rest] = arr` binds array elements
positionally with an optional trailing `..rest` (a fresh `[T; N - k]` remainder) or
bare `..` to ignore it. A rest-less array pattern must match the array's length
exactly. See [Variables → Destructuring](variables.md#destructuring).

## References

- [Variables](variables.md) - Variable declaration and usage
- [Functions](functions.md) - Function types and signatures
- [Operators](operators.md) - Type requirements for operators
- [Expressions](expressions.md) - Expression type checking

## See Also

- Rust Book: [Data Types](https://doc.rust-lang.org/book/ch03-02-data-types.html)
- [Type System Design](https://en.wikipedia.org/wiki/Type_system)
