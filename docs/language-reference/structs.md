# Structs and Enums

Structs group named fields into one nominal type, and enums hold exactly one of several named variants. Both take `impl` blocks for methods, associated functions and trait impls, and both move unless they are `Copy`. Matching on an enum's variants is covered in [Control Flow](control-flow.md#pattern-matching).

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

Fields are declared as `name: Type` and separated by commas. Trailing commas are allowed. A field may have any type, including another struct or an enum. A struct cannot hold itself: naming it in one of its own fields is an error.

Structs are nominal: two structs with identical fields are distinct types, and a value of one does not pass where the other is expected. A struct that holds a reference declares a lifetime parameter for it, as in `struct View<'a> { text: &'a string }` (see [Lifetimes](ownership.md#lifetimes-and-returned-references)).

### Field visibility

A field is private to the module (the file) that declares its struct unless it carries
`export`. In a single-file program every field is reachable throughout it, so this only
begins to matter once a second file reads your code:

```neuro
export struct Config {
    export host: string,
    export port: i32,
    timeout: i32          // private, internal implementation detail
}
```

`export` is per field, so an exported struct can still hold something back. Another module
cannot read `c.timeout`, assign to it, list it in a literal, or reach it through `..base`,
which is what makes a constructor like `Config::new` the only way in. See
[Modules → Visibility](modules.md#visibility).

## Struct Instantiation

Use `StructName { field: value, ... }` to create a value:

```neuro
val origin = Point { x: 0.0, y: 0.0 }
val rect = Rectangle { width: 10, height: 5 }
```

Field order in the literal does not matter. Every declared field must be provided (`MissingStructField`), each only once (`DuplicateStructField`), and no field may be named that the struct does not declare (`UnknownField`).

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

The base must be the same struct type as the literal (otherwise a type mismatch). When a base is present, omitting fields is **not** a `MissingStructField` error: the base fills them in.

## Field Access

Use dot notation to read a field:

```neuro
val p = Point { x: 3.0, y: 4.0 }
val px = p.x   // 3.0
val py = p.y   // 4.0
```

## Field Mutation

Fields on a `mut` binding can be reassigned, at any depth, with or without a compound
operator. A field is a [place](operators.md#places-what-may-sit-on-the-left), the same as
a binding is:

```neuro
mut cursor = Point { x: 0.0, y: 0.0 }
cursor.x = 5.0
cursor.y += 3.0
```

Mutating a field of a `val` binding is a compile error:

```neuro
val fixed = Point { x: 1.0, y: 2.0 }
// fixed.x = 3.0  // error: cannot assign to field 'x' of immutable binding 'fixed'
```

## Passing Structs to Functions

Structs can be passed as function parameters:

```neuro
struct Rectangle {
    width: i32,
    height: i32
}

func area(r: Rectangle) -> i32 {
    return r.width * r.height
}

func main() -> i32 {
    val rect = Rectangle { width: 6, height: 7 }
    return area(rect)  // 42
}
```

A free function, an associated function or a method may return a struct, so a plain factory function and a `Type::new(...)` constructor both work.

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

## impl Blocks

Use `impl TypeName { ... }` to add methods and associated functions to a struct. An enum takes `impl` blocks the same way (see [Methods and Trait Impls](#methods-and-trait-impls)). Generic structs and their impls are covered in [Generics and Traits](generics-and-traits.md#generic-structs-and-impls), and user traits in [Traits and Impls](generics-and-traits.md#traits-and-impls).

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

The receiver is a shared borrow: the method can read fields but cannot move an owning one
out (see [Consuming Methods](#consuming-methods-self)).

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

### Consuming Methods (`self`)

A bare `self` receiver takes the value: the method owns it, and the caller loses it.
This is what an `into_*` conversion needs, because the method may move a non-`Copy`
field straight out of the receiver instead of cloning it.

```neuro
struct Label {
    text: string
}

impl Label {
    func peek(&self) -> u64 {
        self.text.len()
    }

    func into_text(self) -> string {
        self.text            // moved out: the receiver is this method's to spend
    }
}

func main() -> i32 {
    val label = Label { text: "sensor-a" }
    val size = label.peek()            // borrows: `label` survives
    val text = label.into_text()       // consumes: `label` is gone after this
    return text.len() as i32
}
```

- Calling a consuming method **moves** the receiver. A later use of it is a
  `use of moved value` error, exactly as for a by-value argument to a free function.
- A receiver that is a borrow has nothing to give away: calling a consuming method on a
  `&T` / `&mut T`, or on the `self` of a `&self` / `&mut self` method, is
  `cannot move out of`. Bind a `.clone()` first, or take the value by a binding that owns it.
- The receiver is destroyed when the method returns, so a consuming method may not return a
  reference into it.
- On a **`Copy`** struct a `self` receiver duplicates rather than moves, so the caller keeps
  its value. This is what lets an operator-trait method `func add(self, ...)` work.
- A `Drop` type's destructor runs at the method's exit, once ([Destructors](ownership.md#destructors-impl-drop)).

See [`examples/ownership/consuming_self.nr`](../../examples/ownership/consuming_self.nr).

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

### `Self`

Inside an `impl` block, `Self` names the type being extended. It goes wherever that type's
name goes: parameter and return types, local annotations, struct literals, `Self::func()`
calls, and enum variants in expressions and patterns.

```neuro
enum Shape {
    Square(i32),
    Empty
}

impl Shape {
    func unit() -> Self { Self::Square(1) }

    func side(&self) -> i32 {
        match self {
            Self::Square(s) => s,
            Self::Empty => 0
        }
    }
}
```

In `impl<T> Cell<T>`, `Self` is `Cell<T>`. `Self` outside an `impl` or a `trait` is an error.

## Derived Traits (`@derive`)

A struct opts into compiler-generated impls with `@derive(...)`. The derivable set is `Copy`,
`Clone`, `Debug`, `PartialEq` and `Hashable`. Any other name in the list is `UnknownDerive`
rather than a silent no-op, and a repeated name is `DuplicateDerive`. Enums take no `@derive`
(see [Methods and Trait Impls](#methods-and-trait-impls)).

```neuro
@derive(Debug, PartialEq)
struct Sensor { id: i32, label: string }

@derive(Debug, PartialEq)
struct Reading { sensor: Sensor, value: f64 }

func main() -> i32 {
    val a = Reading { sensor: Sensor { id: 7, label: "intake" }, value: 21.5 }
    val b = Reading { sensor: Sensor { id: 7, label: "intake" }, value: 21.5 }
    println("{a:?}")            // Reading { sensor: Sensor { id: 7, label: "intake" }, value: 21.5 }
    if a == b { return 1 }      // structural equality, recursing into `Sensor`
    return 0
}
```

Each derive recurses into the struct's fields, and every field must support the same derive:

- `Debug` gives the `{value:?}` rendering: the struct's name, then each field in declaration
  order, as in `Point { x: 1, y: 2 }`. A field-less struct renders as its bare name. A struct
  has no display form, so `"{p}"` is an error even with the derive; write `"{p:?}"`.
- `PartialEq` gives `==` and `!=`, compared field by field. `PartialEq` is the
  [operator trait](operators.md#operator-overloading) behind `==`.
- `Hashable` hashes the fields in declaration order, so a struct that derives both
  `PartialEq` and `Hashable` is a `HashMap` key with no hand-written impls. A float field is
  refused (`DeriveFieldUnsupported`).
- `Clone` gives `.clone()`, which returns a fresh value without moving the receiver. Each
  field must be `Copy`, or a struct that derives or implements `Clone`, or a type with a
  hand-written `Clone`. A `string`, `Vec`, tensor or `StringBuilder` field is refused
  (`DeriveFieldUnsupported`), so such a struct writes `impl Clone` by hand
  ([BUG-098](../BUGS.md)).
- `Copy` makes the struct copy, rather than move, on bind, assign, return and by-value
  pass. Each field must be `Copy`: scalars, arrays and tuples of them, and structs that derive
  `Copy`. A `string` or collection field is refused, and the error names the field
  (`CopyDeriveNonCopyField`). A struct without `Copy` is a move type; see
  [Move Semantics](ownership.md#moves). `@derive(Copy)` also derives `Clone`,
  and `.clone()` on a `Copy` value always copies the whole value.

A field whose type is a struct or enum uses that type's own derive or hand-written impl of the
trait. A newtype field uses the newtype's own impl when it has one, and its inner value otherwise.
Deriving a trait and also writing its `impl` for the same struct is `DeriveConflictsWithImpl`:
keep one. A struct that derives `Debug` over a field whose type has no debug form is
`DeriveFieldUnsupported`.

See [`examples/structs/derives.nr`](../../examples/structs/derives.nr).

### Derivable traits by hand and as bounds

Each derivable name is also an ordinary trait. The prelude declares `Copy` (which requires
`Clone`), `Clone` (`clone(&self) -> Self`), `Debug` (`fmt_debug(&self, out: &mut StringBuilder)`)
and `Hashable` (`hash(&self) -> u64`).

A struct may implement any of them by hand. `{x:?}` calls a hand-written `fmt_debug`, and a
hand-written `impl Copy` is accepted only where `@derive(Copy)` would be: on a struct whose
fields are all `Copy`, with no `Drop`, and with `Clone` implemented as well. On a generic struct,
`impl<T: Copy> Copy for Cell<T> {}` may bound its parameters by `Copy` and nothing else.
Derived or written, a generic struct's `Copy` holds per instance: `Cell<i32>` copies, and
`Cell<string>`, whose field is not `Copy`, moves.

```neuro
struct Money { cents: i64 }

impl Debug for Money {
    func fmt_debug(&self, out: &mut StringBuilder) {
        out.push_str("${self.cents / 100}.{self.cents % 100:02}")
    }
}

impl Clone for Money {
    func clone(&self) -> Self { Money { cents: self.cents } }
}

impl Copy for Money {}
```

Any of the five names may bound a type parameter. A derive, a hand-written impl and a builtin
type meet the bound alike: the scalars implement all five (a float is not `Hashable`), `string`
all but `Copy`, and every `Copy` type is `Clone`. So one generic body serves all of them:

```neuro
func label<T: Debug>(name: string, v: &T) -> string {
    "{name} = {v:?}"            // a derived struct, `Money`, or an `i32`
}
```

A `T: PartialEq` body may call `v.eq(&w)`, and a `T: Hashable` body may call `v.hash()`. What a
`T: Copy` or `T: Clone` bound gives a generic body is in
[Generics and Traits](generics-and-traits.md#ownership-through-a-type-parameter). See
[`examples/structs/derivable_traits.nr`](../../examples/structs/derivable_traits.nr).

## Struct Errors

| Error | Trigger |
|---|---|
| `UnknownStruct` | Using an undefined struct name |
| `StructAlreadyDefined` | Redefining a struct |
| `UnknownField` | Naming a field the struct does not declare, in a literal, an access or an assignment |
| `MissingStructField` | Omitting a field in a struct literal |
| `DuplicateStructField` | Providing the same field twice in a literal |
| `AssignToImmutableField` | Mutating a field on a `val` binding |
| `MethodNotFound` | Calling a method that doesn't exist on the type |
| `UnknownTrait` | Implementing a trait that was never declared |
| `MissingTraitMethod` | A trait impl omits a required method |
| `NotATraitMethod` | A trait impl defines a method the trait does not declare |
| `TraitMethodSignatureMismatch` | An impl method's signature differs from the trait's |
| `TraitBoundNotSatisfied` | A generic argument does not implement a required trait |
| `MethodGenericShadowsImpl` | A generic method's parameter reuses a name its `impl` declares |
| `GenericMethodInTraitImpl` | A method of a trait `impl` declares type parameters of its own |
| `UnknownDerive` | A `@derive` argument that names no derivable trait |
| `DuplicateDerive` | The same trait listed twice in a `@derive` list |
| `DeriveFieldUnsupported` | A field a derive cannot render, compare, clone or hash |
| `CopyDeriveNonCopyField` | `@derive(Copy)` or `impl Copy` on a struct with a field that is not `Copy` |
| `DeriveConflictsWithImpl` | A struct both derives a trait and declares an `impl` of it |
| `InvalidCopyImpl` | `impl Copy` on an enum or a newtype, or a generic `impl Copy` bounding a parameter by another trait |
| `MissingSupertraitImpl` | `impl Copy` without `Clone`, or `impl Comparable` without `PartialEq` |
| `UnrenderableStruct` | A struct hole with no `Debug`, derived or by hand, or missing the `:?` specifier |

## Enum Types

An enum holds exactly one of several named **variants**. A variant may be a bare tag, carry a
positional **tuple** payload, or carry **named fields**, and all three may appear in one enum.
Like structs, enums use nominal typing.

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

An enum value can be bound to a `val` or `mut`, passed to and returned from functions, and stored
in a struct field. It is **`Copy`** exactly when every payload it can carry is, so an enum over
scalars duplicates on a bind while one carrying a `string` moves.

### Construction

Each variant shape has its own construction syntax, all prefixed with the enum name:

```neuro
val c = Color::Red                       // unit variant
val m = Message::Move(1, 2)              // tuple variant
val s = Shape::Circle { radius: 5.0 }    // struct variant
```

### Methods and Trait Impls

An enum takes `impl` blocks as a struct does: `&self`, `&mut self` and consuming `self`
methods, associated functions, user trait impls (static dispatch through a bound, dynamic
through `&dyn Trait`, default methods included) and operator traits such as `PartialEq`. The
body usually `match`es on `self`, and a `&mut self` method may assign a whole new variant to
`self`. From [`examples/types/enum_methods.nr`](../../examples/types/enum_methods.nr):

```neuro
enum Light {
    Red,
    Amber,
    Green
}

impl Light {
    func start() -> Light {
        return Light::Red
    }

    func wait_secs(&self) -> i32 {
        match self {
            Light::Red => 30,
            Light::Amber => 3,
            Light::Green => 25
        }
    }

    func advance(&mut self) {
        self = match self {
            Light::Red => Light::Green,
            Light::Green => Light::Amber,
            Light::Amber => Light::Red
        }
    }
}

impl PartialEq for Light {
    func eq(&self, other: &Light) -> bool {
        return self.wait_secs() == other.wait_secs()
    }
}
```

Enums differ from structs in three ways:

- There is no `@derive` on an enum. Write `impl Debug`, `impl PartialEq` or `impl Clone` by hand.
- `impl Copy` is refused. An enum is `Copy` by its payloads, as described under Definition.
- An `impl` member may not share a variant's name (`ImplMemberNamesVariant`): `Light::start()`
  and `Light::Red` must stay distinct. A generic enum's `impl<T> Tree<T>` is instantiated for each
  instance, as a generic struct's is.

### Memory Layout

An enum is a tagged union: a discriminant identifies the active variant, and the storage is sized
for the widest variant's payload. Two enums with the same variant names but declared separately
are distinct types.

### Generic Enums

An enum may take type parameters. Each distinct set of type arguments is monomorphized into its own
nominal tagged union, exactly as a [generic struct](generics-and-traits.md#generic-structs-and-impls)
is, so `Slot<i32>` and `Slot<i64>` are different types, each with its own payload width, and there
is no runtime cost.

```neuro
enum Slot<T> { Filled(T), Vacant }
enum Tagged<T, U> { Left(T), Right(U) }

func main() -> i32 {
    val a = Slot::Filled(4)                  // T inferred from the payload → Slot<i32>
    val b: Slot<bool> = Slot::Vacant         // no payload, so the annotation fixes T
    val t: Tagged<i32, bool> = Tagged::Right(true)
    return 0
}
```

Where the type arguments come from, in order:

1. **The expected type**: an annotated binding, a parameter, a struct field, a `return`.
2. **The payload**: `Slot::Filled(4)` determines `T = i32` by unifying `4`'s type against the variant's declared payload.
3. **The enclosing function's return type**, when it is an instance of the same enum. This is what makes the common fallible-function shape work, because a tail `if` branch has no other context:

```neuro
func divide(a: i32, b: i32) -> Result<i32, i32> {
    if b == 0 {
        Result::Err(1)                   // Err determines E; T comes from the return type
    } else {
        Result::Ok(a / b)                // Ok determines T; E comes from the return type
    }
}
```

If none of the three determines an argument, the construction is rejected with `GenericEnumNotInferable`: annotate the target. Using the bare name as a type (`func f(o: Slot) -> i32`) is `GenericEnumNeedsArgs`.

A `match` pattern is written with the **base** name. It matches whatever instance the scrutinee has, and its payload binds at that instance's concrete type:

```neuro
match a {
    Slot::Filled(v) => v + 1,            // v is i32 here
    Slot::Vacant => 0
}
```

### `Option<T>` and `Result<T, E>`

These two generic enums are the standard library's absence and failure types. They are available in **every program without a declaration**, because the compiler prepends an implicit prelude:

```neuro
enum Option<T> { Some(T), None }
enum Result<T, E> { Ok(T), Err(E) }
```

They are ordinary generic enums: nothing about them is special-cased in the type checker or the backend, and a program that declares its own `Option` or `Result` shadows the prelude entry.

```neuro
func unwrap_or(o: Option<i32>, fallback: i32) -> i32 {
    match o {
        Option::Some(v) => v,
        Option::None => fallback
    }
}

func main() -> i32 {
    val present = Option::Some(30)
    val absent: Option<i32> = Option::None
    return unwrap_or(present, 0) + unwrap_or(absent, 12)  // 42
}
```

Variants may be written qualified (`Option::Some`, `Result::Err`) or, because the implicit prelude imports `Some`, `None`, `Ok`, and `Err` into every file without `@no_prelude`, unqualified (`Some(x)`). The [`??` operator](operators.md#nullerror-coalescing-operator-) unwraps either type with a fallback, the [`?` operator](operators.md#error-propagation-operator-) unwraps one or hands the failure to the caller, and [`val-else`](control-flow.md#val-else-unwrap-or-leave-the-scope) unwraps one or exits the scope.

#### Methods

Both enums carry a few methods, written as ordinary Neuro source in the prelude. Each one
consumes its receiver, except the `is_*` tests, which borrow it.

| Method | On | Result |
|---|---|---|
| `.map(f)` | `Option<T>`, `Result<T, E>` | `f: (T) -> U` applied to the `Some` / `Ok` payload |
| `.map_err(f)` | `Result<T, E>` | `f: (E) -> F` applied to the `Err` payload |
| `.ok()` | `Result<T, E>` | `Option<T>`, the error discarded |
| `.ok_or(e)` | `Option<T>` | `Result<T, E>`, with `e` for `None` |
| `.is_some()` / `.is_none()` | `Option<T>` | `bool` |
| `.is_ok()` / `.is_err()` | `Result<T, E>` | `bool` |

```neuro
val doubled = find(2).map(|v| v * 2) ?? 0
val checked: Result<i32, string> = find(0).ok_or("not found")
val code = divide(1, 0).map_err(|msg| msg.len())
if checked.is_err() { println("missing") }
```

`f` takes any closure or function value, so a tuple variant can wrap an error in one step:
`read(path).map_err(AppError::Io)?`. There is no `.unwrap()`: `?? panic("...")` unwraps.
A program that declares its own `Result` keeps `Option`'s methods except `.ok_or`, which builds
the prelude's `Result`; a program with its own `Option` loses `Result::ok` the same way.

### Limitations

- Payloads must be sized. Any sized type is admissible, `Copy` or not: a `string`, a struct, an array, a tuple, another enum. `void` and the unsized types (`dyn Trait`, `[T]`) are rejected (`UnsupportedEnumPayload`), because a payload slot has to have a width.
- No enum that holds itself inline, directly or through a struct, array, tuple or another enum: it would have no finite size.
- No `impl Drop` on an enum (`InvalidDropImpl`): only a struct runs a user destructor, see [Destructors](ownership.md#destructors-impl-drop).
- **A generic enum's tuple variant is a function value only where a function type is
  expected.** `Error::Io` passes as a function anywhere. `Option::Some` names no instance until
  something says which, so it needs a function type from its context: an annotation
  (`val f: (i32) -> Option<i32> = Option::Some`), a parameter (`apply(Wrap::Two)`), or a
  generic call whose earlier inputs fixed its parameters (`o.map(Option::Some)` with
  `o: Option<i32>`). Bare, `val f = Option::Some` is an error; write `|x| Option::Some(x)`.

### Enum Errors

| Error | Cause |
|---|---|
| `EnumAlreadyDefined` | Two `enum` declarations share a name |
| `UnknownEnumVariant` | Construction names a variant the enum does not declare |
| `EnumVariantFormMismatch` | A variant built with the wrong syntax (e.g. a struct variant called like a function) |
| `EnumVariantArityMismatch` | A tuple variant built with the wrong number of arguments |
| `UnknownEnumField` / `MissingEnumField` / `DuplicateEnumField` | Struct-variant field set is wrong |
| `UnsupportedEnumPayload` | A variant payload is `void` or an unsized type |
| `GenericEnumNeedsArgs` | A generic enum's bare name used as a type |
| `GenericEnumNotInferable` | A construction whose type arguments no context determines |
| `GenericArgCountMismatch` | `Option<i32, bool>`, wrong number of type arguments |
| `ImplMemberNamesVariant` | An `impl` method or associated function named like one of the enum's variants |
| `InvalidDropImpl` | `impl Drop` on an enum |

## See also

- [Types](types.md): the type system overview
- [Variables](variables.md): `val` and `mut` bindings
- [Functions](functions.md): function definitions
- [Generics and Traits](generics-and-traits.md): generic structs and enums, trait impls and bounds
- [Ownership and Borrowing](ownership.md): moves, `Copy`, destructors and struct lifetimes
- [Control Flow](control-flow.md): `match` and pattern matching
- [Modules](modules.md): field and declaration visibility
