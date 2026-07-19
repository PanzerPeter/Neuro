# Structs and Methods

Structs are user-defined types that group related fields together. Methods add behaviour to structs via `impl` blocks.

## Struct Definition

```neuro
struct Point {
    x: f64,
    y: f64
}

struct Rectangle {
    width: i32,
    height: i32
}
```

Fields are declared as `name: Type` and separated by commas. Trailing commas are allowed.

## Struct Instantiation

Use `StructName { field: value, ... }` to create a value:

```neuro
val origin = Point { x: 0.0, y: 0.0 }
val rect = Rectangle { width: 10, height: 5 }
```

All fields must be provided. Field order in the literal does not need to match the definition order.

### Field-Init Shorthand

When a local variable has the same name as the field it initialises, write the name once:

```neuro
val x = 1.0
val y = 2.0
val p = Point { x, y }   // equivalent to Point { x: x, y: y }
```

Shorthand and explicit `field: value` entries may be mixed: `Point { x, y: 2.0 }`. A shorthand field references the same-named binding in scope; if no such binding exists, it is an ordinary undefined-name error.

### Functional Update (`..base`)

A trailing `..base` supplies every field not listed explicitly from an existing value of the same struct type. Listed fields override the base:

```neuro
val p = Point { x: 1.0, y: 2.0 }
val shifted = Point { x: 10.0, ..p }   // x = 10.0, y inherited from p (2.0)
val copy = Point { ..p }               // all fields copied from p
```

The base must be the same struct type as the literal (otherwise a type `Mismatch` error). When a base is present, omitting fields is **not** a `MissingStructField` error — the base fills them in. The base is evaluated and its fields are copied into the new value; no allocation is introduced.

## Field Access

Use dot notation to read a field:

```neuro
val p = Point { x: 3.0, y: 4.0 }
val px = p.x   // 3.0
val py = p.y   // 4.0
```

## Field Mutation

Fields on a `mut` binding can be reassigned:

```neuro
mut cursor = Point { x: 0.0, y: 0.0 }
cursor.x = 5.0
cursor.y = 3.0
```

Mutating a field of a `val` binding is a compile error:

```neuro
val fixed = Point { x: 1.0, y: 2.0 }
// fixed.x = 3.0  // Error: AssignToImmutableField
```

## Passing Structs to Functions

Structs can be passed as function parameters:

```neuro
func area(r: Rectangle) -> i32 {
    return r.width * r.height
}

func main() -> i32 {
    val rect = Rectangle { width: 6, height: 7 }
    return area(rect)  // 42
}
```

> **Phase 1 limitation**: Structs as function *return* types are not yet supported.

## impl Blocks

Use `impl TypeName { ... }` to add methods and associated functions to a struct.

### Instance Methods (`&self`)

Instance methods take `&self` as their first parameter and can read any field:

```neuro
struct Counter {
    value: i32
}

impl Counter {
    func get(&self) -> i32 {
        self.value
    }

    func add(&self, n: i32) -> i32 {
        self.value + n
    }
}

func main() -> i32 {
    val c = Counter { value: 10 }
    return c.add(32)  // 42
}
```

- `self` inside the method refers to the receiver struct value.
- All struct fields are accessible via `self.field`.
- The receiver is passed by value (read-only snapshot).

### Mutating Methods (`&mut self`)

A method may take `&mut self` to mutate the receiver in place. Writes to
`self.field` propagate back to the caller's value because the receiver is passed
by pointer:

```neuro
struct Accumulator {
    total: i32
}

impl Accumulator {
    func add(&mut self, n: i32) {
        self.total = self.total + n
    }

    func get(&self) -> i32 {
        self.total
    }
}

func main() -> i32 {
    mut acc = Accumulator { total: 0 }
    acc.add(10)
    acc.add(32)
    return acc.get()  // 42
}
```

- Calling a `&mut self` method requires a `mut` receiver (or one reached through a
  `&mut T`). Calling it on a `val` binding is a `cannot mutably borrow` error.
- The call takes an **exclusive** borrow of the receiver for its duration, so it is
  rejected while another borrow of that receiver is live (aliasing rule).

### Associated Functions (no `self`)

Associated functions belong to the type but do not take a receiver. They are called via `TypeName::func(args)`:

```neuro
struct Point {
    x: i32,
    y: i32
}

impl Point {
    func new(x: i32, y: i32) -> Point {
        Point { x: x, y: y }
    }
}

func main() -> i32 {
    val p = Point::new(10, 32)
    return p.x + p.y  // 42
}
```

### Combining Methods and Associated Functions

```neuro
struct Rect {
    width: i32,
    height: i32
}

impl Rect {
    func new(w: i32, h: i32) -> Rect {
        Rect { width: w, height: h }
    }

    func area(&self) -> i32 {
        self.width * self.height
    }
}

func main() -> i32 {
    val r = Rect::new(6, 7)
    return r.area()  // 42
}
```

## Destructors (`impl Drop`)

A struct can define a destructor by implementing the built-in `Drop` trait. Its
`drop(&mut self)` method runs automatically when an owner of the value goes out
of scope:

```neuro
struct Guard {
    sink: &mut i32
}

impl Drop for Guard {
    func drop(&mut self) {
        *self.sink = *self.sink + 1   // record that the destructor ran
    }
}

func main() -> i32 {
    mut dropped: i32 = 0
    {
        val g = Guard { sink: &mut dropped }
    }                 // `g` goes out of scope here → `drop` runs, dropped == 1
    return dropped    // 1
}
```

Rules:

- Destructors run **only on normal scope exit** — fall-through, `return`,
  `break`, and `continue`. A panic aborts the process without running any
  destructor (there is no stack unwinding).
- When several owned values leave the same scope, they are dropped in **reverse
  declaration order** (LIFO).
- A value that has been **moved** out (rebound, returned, passed by value, or
  stored into a struct) is dropped exactly once, by its final owner — never
  twice. Reading a moved value is already a compile error.
- A `Copy` type may **not** implement `Drop` (a type with a destructor is moved,
  not duplicated). `@derive(Copy)` together with `impl Drop` is a compile error.
- The `drop` method must be exactly `drop(&mut self)` — no extra parameters and
  no return type — and an `impl Drop` block may contain no other methods.

Not yet supported: reassigning a `Drop` binding does not run the prior value's
destructor, and a struct's `Drop`-typed fields are not dropped automatically
(no recursive destructor glue).

## Definition Order Independence

Structs and `impl` blocks can appear in any order. Forward references are supported:

```neuro
impl Score {
    func doubled(&self) -> i32 {
        self.value * 2
    }
}

struct Score {
    value: i32
}

func main() -> i32 {
    val s = Score { value: 21 }
    return s.doubled()  // 42
}
```

## Generic Structs and Impls

A struct may declare type parameters in `<...>` after its name; each distinct set of
concrete type arguments is monomorphized into its own specialized struct at zero runtime
cost. Type arguments are **inferred** from the field values at a struct literal, or
written explicitly in a type annotation (`Pair<i32, f64>`).

```neuro
struct Pair<T, U> {
    first: T,
    second: U
}

// A generic inherent impl: `impl<T> Wrapper<T>` specializes its methods per instance.
struct Wrapper<T> {
    value: T
}

impl<T> Wrapper<T> {
    func get(&self) -> T {
        self.value
    }
}

func first_of(p: &Pair<i32, i32>) -> i32 {
    p.first
}

func main() -> i32 {
    val p = Pair { first: 40, second: 2 }   // Pair<i32, i32> inferred
    val w = Wrapper { value: 30 }           // Wrapper<i32>
    val flag = Wrapper { value: true }      // Wrapper<bool> — distinct instance
    return first_of(&p) + w.get()           // 40 + 30 = 70
}
```

**Restrictions (this phase).** Type arguments are restricted to `Copy` types (a bare type
parameter has no move semantics yet). A generic struct is usable only *with* type arguments
— its bare name is rejected. A generic instantiated with an enclosing type parameter (a
`Wrapper<T>` field inside another generic struct) is a documented limitation, deferred with
broader generic support.

### Const (value) parameters

A generic struct may also declare a **const parameter** — a compile-time value used as an array
length, so `[T; CAP]` is sized concretely per instance:

```neuro
struct Buffer<T, const CAP: u32> {
    data: [T; CAP],
    count: u32
}

// CAP is inferred from the field array's length (Buffer<i32, 4>):
val buf = Buffer { data: [1, 2, 3, 4], count: 4 }

// or written explicitly in a type annotation:
val other: Buffer<i32, 4> = Buffer { data: [5, 6, 7, 8], count: 4 }
```

Each distinct `CAP` produces its own monomorphized struct at zero runtime cost. A generic `impl`
over a struct's const parameter is a documented limitation deferred to broader generic support.

## Traits

A `trait` defines shared behaviour that many types can implement. Traits are Neuro's
mechanism for bounded polymorphism. A trait method is either **required** (a signature
with no body — implementors must provide one) or a **default** method (a signature with a
body that implementors inherit unless they override it).

```neuro
trait Shape {
    // Required: every implementor must define this.
    func area(&self) -> i32

    // Default (provided): inherited unless the implementor writes its own.
    func is_big(&self) -> i32 {
        if self.area() > 20 { 1 } else { 0 }
    }
}
```

Implement a trait for a type with `impl Trait for Type`:

```neuro
struct Square { side: i32 }

impl Shape for Square {
    func area(&self) -> i32 { self.side * self.side }
    // `is_big` is not written, so Square inherits the trait's default.
}
```

The compiler checks each trait impl for **conformance**: every required method must be
present, each method's signature must match the trait's, and an impl may only contain
methods the trait declares.

### Trait bounds on generics

A generic parameter may be bounded by a trait (`<T: Shape>`). Inside the body, the trait's
methods may be called on the bounded parameter; at the call site the concrete type
argument must implement the trait:

```neuro
func scaled_area<T: Shape>(s: &T, factor: i32) -> i32 {
    s.area() * factor          // dispatched through the `Shape` bound
}
```

Traits are **fully monomorphized and erased**: each `impl` lowers to ordinary methods and
each trait-bounded generic is specialized per concrete type, so there is no vtable and no
runtime cost. Supertraits, associated types, dynamic dispatch (`dyn`), and the operator
traits land later in sub-phase 1F.

## Unsupported (Phase 1+)

The following are not yet implemented and will be rejected at compile time:

- `self` (consuming) — needs the by-value struct ABI
- Struct return types from free functions (backend limitation; associated functions and methods may return structs)
- Nested structs as field types

```neuro
// Consuming `self` is still rejected with a clear error:
impl Foo {
    func consume(self) { ... }      // Error: UnsupportedSelfParam
}
```

## Nominal Typing

Neuro uses nominal typing for structs: two struct types are compatible only if they have the same name, regardless of field layout.

## Error Types

| Error | Trigger |
|---|---|
| `UnknownStruct` | Using an undefined struct name |
| `StructAlreadyDefined` | Redefining a struct |
| `UnknownField` | Accessing a field that doesn't exist |
| `MissingStructField` | Omitting a field in a struct literal |
| `DuplicateStructField` | Providing the same field twice in a literal |
| `AssignToImmutableField` | Mutating a field on a `val` binding |
| `MethodNotFound` | Calling a method that doesn't exist on the type |
| `UnsupportedSelfParam` | Using consuming `self` (by value) in a method |
| `UnknownTrait` | Implementing a trait that was never declared |
| `MissingTraitMethod` | A trait impl omits a required method |
| `NotATraitMethod` | A trait impl defines a method the trait does not declare |
| `TraitMethodSignatureMismatch` | An impl method's signature differs from the trait's |
| `TraitBoundNotSatisfied` | A generic argument does not implement a required trait |

## References

- [Types](types.md) — type system overview
- [Variables](variables.md) — `val` and `mut` bindings
- [Functions](functions.md) — function definitions
