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

> **Phase 2 limitation**: Structs as function *return* types are not yet supported.

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

A method may take `&mut self` to mutate the receiver in place (§2.5). Writes to
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
  rejected while another borrow of that receiver is live (§2.5 aliasing rule).

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

## Unsupported (Phase 2+)

The following are not yet implemented and will be rejected at compile time:

- `self` (consuming) — needs the by-value struct ABI
- Struct return types from free functions (backend limitation; associated functions and methods may return structs)
- Nested structs as field types
- Generics on structs

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

## References

- [Types](types.md) — type system overview
- [Variables](variables.md) — `val` and `mut` bindings
- [Functions](functions.md) — function definitions
