# Generics and Traits

Generic parameters let one function, struct, enum or impl serve many types, and traits name the behaviour a type must provide to be used that way. Each concrete use is compiled into its own specialized copy, so generics and trait bounds cost nothing at runtime. `dyn Trait`, covered at the end, is the one form that dispatches through a vtable.

## Generic Functions

A function may declare **type parameters** in angle brackets after its name. The compiler generates one specialized copy of the function per distinct set of concrete type arguments (**monomorphization**), so a type parameter carries zero runtime cost.

```neuro
// A single template, instantiated at each concrete type it is called with.
func identity<T>(x: T) -> T {
    x
}

// Multiple type parameters.
func second<T, U>(a: T, b: U) -> U {
    b
}

func main() -> i32 {
    val a = identity(41)     // identity<i32>
    val f = identity(2.5)    // identity<f64>, a separate specialized copy
    val s = second(1.5, 7)   // second<f64, i32> -> 7
    return a + s             // 41 + 7 = 48
}
```

**What a generic body may do.** The body is checked once, before any type argument is known, so an unbounded type parameter supports only operations valid for any type: binding a value, returning it, passing it to another function, and building or observing tuples. Operations that need a concrete type (arithmetic, comparison, field access, method calls) are rejected unless a [bound](#trait-bounds-and-where-clauses) supplies them:

```neuro
func bad<T>(a: T, b: T) -> T {
    a + b   // error: cannot apply binary operator + to types T and T
}
```

### Generic types at the function's parameter

A generic struct or enum may take the function's own parameter as its argument, in the signature and in the body. The call infers `T` through the instance it is handed, so `unwrap_or(first_some(40), 0)` below is an `i32`:

```neuro
func first_some<T>(x: T) -> Option<T> {
    Option::Some(x)
}

func unwrap_or<U>(o: Option<U>, d: U) -> U {
    match o {
        Option::Some(v) => v,
        Option::None => d,
    }
}

func main() -> i32 {
    return unwrap_or(first_some(40), 0)   // 40
}
```

A generic that calls itself with a type built from its own parameter (`grow((x, 1))` inside `grow<T>`) would need a new instance at every level. The compiler stops once the chain passes 128 instances deep: ``instantiating 'grow' nests generic instances more than 128 deep``.

### Ownership through a type parameter

Because the body is checked once for every instantiation at once, a type parameter is treated as **non-`Copy`**. A `T`-typed binding moves when it is placed into a new owner, and reading it afterwards is an error even where the call site supplies `i32`. The same holds for `(v, v)` and `[v, v]`.

```neuro
func dup<T>(v: T) -> T {
    val a = v
    val b = v        // error: use of moved value 'v': bind a `.clone()` if you need an independent copy
    b
}
```

Because the body already holds for the most restrictive case, a non-`Copy` type argument needs no further check. The value is passed and returned by value, and a [`Drop`](ownership.md#destructors-impl-drop) type's destructor runs exactly once per value, whether the callee consumes it or hands it back.

```neuro
struct Holder {
    name: string
}

func identity<T>(x: T) -> T {
    x
}

func main() -> i32 {
    val h = identity(Holder { name: "held" })   // identity<Holder>: moved in, moved back out
    val s = identity("text")                    // identity<string>
    val n = identity(41)                        // identity<i32>
    return n
}
```

To copy a `T` instead, give it a `Copy` or `Clone` bound (see [Copy and Clone bounds](#copy-and-clone-bounds)).

## Type Inference and Turbofish

Type arguments are inferred from the call's value arguments. A type parameter that no argument mentions cannot be inferred, and the call must supply it with a **turbofish**, `::<...>`:

```neuro
func zero<T>() -> i32 { 0 }

func main() -> i32 {
    return zero()   // error: generic parameter 'T' cannot be inferred from the call arguments; supply it explicitly with a turbofish, e.g. `f::<...>(...)`
}
```

The turbofish is the only call-site form for explicit generic arguments, and its arguments may be types or const values. A turbofish on a function that declares no type parameters is an error (`turbofish supplies 1 generic argument(s), but 'identity' declares 0`).

```neuro
func identity<T>(x: T) -> T { x }
func count<const N: u32>() -> u32 { N }

func main() -> i32 {
    val a = identity::<i32>(5)   // explicit type argument
    val b = count::<4>()         // explicit const argument
    return a + (b as i32)        // 5 + 4 = 9
}
```

## Const (Value) Parameters

A parameter list may declare a **const parameter**, `const NAME: T`, which is a compile-time value of an integer type. It can size an array and be read as a value in the body. Each distinct value is monomorphized into its own specialized code. A function's const parameter is inferred from the length of an array argument:

```neuro
func sum<const N: u32>(a: [i32; N]) -> i32 {
    mut total: i32 = 0
    for x in a {
        total = total + x
    }
    total          // N is inferred from the array's length
}

func main() -> i32 {
    val xs: [i32; 3] = [10, 20, 12]
    return sum(xs)   // sum<3> -> 42
}
```

A struct takes a const parameter the same way, and its field array is sized per instance:

```neuro
struct Buffer<T, const CAP: u32> {
    data: [T; CAP],
    count: u32
}

impl<T, const CAP: u32> Buffer<T, CAP> {
    func capacity(&self) -> u32 { CAP }
}

func main() -> i32 {
    // CAP inferred from the field array's length: Buffer<i32, 4>.
    val buf = Buffer { data: [1, 2, 3, 4], count: 4 }
    // Or written explicitly in a type annotation.
    val other: Buffer<i32, 4> = Buffer { data: [5, 6, 7, 8], count: 4 }
    return (buf.capacity() + other.count) as i32   // 4 + 4 = 8
}
```

A `where` clause can constrain a const parameter; see [Trait Bounds and Where Clauses](#trait-bounds-and-where-clauses).

## Generic Structs and Impls

A struct may declare type parameters after its name, and each distinct set of type arguments is its own specialized struct. Type arguments are inferred from the field values at a struct literal, or written in a type annotation (`Pair<i32, f64>`). Enums take parameters the same way (see [Generic Enums](structs.md#generic-enums)).

```neuro
struct Pair<T, U> {
    first: T,
    second: U
}

struct Cell<T> {
    v: T
}

// `impl<T> Cell<T>` specializes its methods per instance.
impl<T> Cell<T> {
    func new(v: T) -> Cell<T> {
        Cell { v: v }
    }

    func get(self) -> T {   // by value: a `&self` method cannot move `self.v` out of the borrow
        self.v
    }
}

func first_of(p: &Pair<i32, i32>) -> i32 {
    p.first
}

func main() -> i32 {
    val p = Pair { first: 40, second: 2 }   // Pair<i32, i32> inferred
    val c = Cell::new(30)                   // Cell<i32>, inferred from the argument
    val flag = Cell { v: true }             // Cell<bool>, a distinct instance
    return first_of(&p) + c.get()           // 40 + 30 = 70
}
```

A type argument carries no `Copy` requirement. An instance holds its value and moves with it, so `Cell<string>` owns the string it holds (see [Ownership of an element](ownership.md#ownership-of-elements)). A generic struct is usable only with type arguments:

```neuro
func f(p: Pair) -> i32 { 0 }   // error: generic struct 'Pair' requires type arguments, e.g. `Pair<...>`
```

A field, an impl signature or a body may instantiate a generic with a parameter in scope. `struct Outer<U> { i: Inner<U> }` holds an `Inner<i32>` in an `Outer<i32>`. Inside `impl<T> Cell<T>`, both `Cell<T>` and `Self` are the type `self` has. An associated function infers the instance from its arguments, so `Cell::new(30)` calls `new` on `Cell<i32>`. When the arguments do not mention the parameter, a turbofish supplies it (`Bag::nothing::<i32>()`).

## Generic Methods

A method may declare type parameters of its own, after its name and in addition to its impl's. A call infers them from its arguments. When the arguments do not mention one, a turbofish after the method name supplies it. Each set of type arguments a program uses compiles to its own copy of the method, as for a generic function.

```neuro
impl<T> Cell<T> {
    func with<U>(self, other: U) -> Pair<T, U> {
        Pair { first: self.v, second: other }
    }
}

// Cell::new(5).with("five") infers U = string.
// Cell::new(true).with::<i64>(40i64) takes U = i64 from the turbofish.
```

An associated function takes its own parameters the same way. On a generic type, one turbofish lists the type's parameters first and the function's own after them: `Cell::empty::<i32, f64>()`. A method's parameter may not reuse a name its impl declares. [`generic_methods.nr`](../../examples/structs/generic_methods.nr) runs these forms.

A trait method declares parameters the same way, and a call through a bound infers them or takes a turbofish:

```neuro
trait Labeler {
    func label<U>(&self, value: U) -> (bool, U)
}

impl Labeler for Shelf {
    func label<V>(&self, value: V) -> (bool, V) {
        (self.left, value)
    }
}

func labeled_value<T: Labeler>(source: &T) -> i32 {
    source.label(7).1
}
```

An implementor declares the same list up to renaming: as many parameters in the same order, each of the same kind, with the same bounds and the same `where` predicates. An `impl Trait` parameter counts as one of them, so either side may spell it either way. A default method carries its parameters into each implementor, where they may not reuse a name of that `impl`. `Drop` and the operator traits declare none. A trait declaring a generic method is not [object-safe](#object-safety).

## Traits and Impls

A `trait` defines behaviour that many types can implement. A trait method is either **required** (a signature with no body, which every implementor must define) or a **default** (a body that implementors inherit unless they override it):

```neuro
trait Shape {
    // Required: every implementor must define this.
    func area(&self) -> i32

    // Default: inherited unless the implementor writes its own.
    func is_big(&self) -> i32 {
        if self.area() > 20 { 1 } else { 0 }
    }
}

struct Square {
    side: i32
}

impl Shape for Square {
    func area(&self) -> i32 { self.side * self.side }
    // `is_big` is not written, so Square inherits the trait's default.
}
```

The compiler checks each trait impl for **conformance**: every required method must be present, each method's signature must match the trait's, and an impl may contain only methods the trait declares. A missing method is reported as ``impl Shape for Square is missing required method 'area'``.

Inside a trait, `Self` names the implementing type. An impl may write `Self` or its own name. A default method that names `Self` means each implementor in turn, so through a bound `T: Resizable` a call to `grown` returns `T`:

```neuro
trait Resizable {
    func grown(&self, by: i32) -> Self
}

impl Resizable for Square {
    func grown(&self, by: i32) -> Self { Self { side: self.side + by } }
}
```

Enums and newtypes take trait impls the same way structs do. A trait that names `Self` outside its receiver cannot be used as `dyn Trait` (see [Object Safety](#object-safety)).

The operator traits (`Add`, `Neg`, `PartialEq`, `Comparable` and the rest) are built into the compiler and are documented under [Operator Overloading](operators.md#operator-overloading). A `T: PartialEq` bound gives a generic body `==` and `!=`, and `T: Comparable` gives it the orderings, each with its method form (`a.le(b)`). An integer or `char` meets `Comparable`; a float does not, because `NaN` has no place in a total order. The arithmetic and `*Assign` traits give a type parameter no operator yet. The derivable traits (`Copy`, `Clone`, `Debug`, `Hashable`) are also ordinary traits, see [Derivable traits](structs.md#derivable-traits-by-hand-and-as-bounds).

## Supertraits

A trait may require other traits, written with the same `: A + B` syntax as a bound. Every `Priced` type is then also `Named`, so a default method can call what `Named` declares:

```neuro
trait Named {
    func name(&self) -> string
}

trait Priced: Named {
    func cents(&self) -> i32

    func label(&self) -> string {
        "{self.name()} costs {self.cents()}"
    }
}
```

`impl Priced for Book` needs `impl Named for Book` as well, or the program is rejected with ``` `impl Priced for Book` requires `impl Named for Book` ```. The two impls may come in either order, and a derive counts as an impl. A supertrait may be any declared trait, derivable trait or operator trait, or `Drop`. An unknown name is `UnknownTrait`, and a trait that reaches itself through its list is `SupertraitCycle`.

A bound brings its supertraits with it, all the way up. Inside `func f<T: Priced>`, `T` has `Named`'s methods and satisfies a `Named` bound. If a supertrait is `Copy`, `T` copies. The prelude's `Copy` is itself one of these: `trait Copy: Clone {}`.

A supertrait may fix an associated type, which then holds for every implementor and types its methods through a bare bound:

```neuro
trait Numbers: Source<Item = u32> {}

func head<T: Numbers>(t: &mut T) -> Option<u32> { t.next() }
```

An impl whose binding differs is `AssociatedTypeBoundMismatch`. A subtrait does not inherit a supertrait's associated types: it cannot write `Self::Item` in its own signatures, and `T: Numbers<Item = u32>` is refused. To constrain a supertrait the list leaves open, name it too, as in `T: Named + Source<Item = u32>`.

A generic impl holds per instance. With `impl<T: Tag> Base for Wrap<T>` and `impl<T> Top for Wrap<T>`, where `Top: Base`, a `Wrap<i32>` meets `Top` only if `i32` is `Tag`.

Two traits on one type cannot share a method name, so a subtrait redeclaring a supertrait's method cannot be implemented beside it. [`supertraits.nr`](../../examples/structs/supertraits.nr) runs these forms.

## Trait Bounds and Where Clauses

A generic parameter may be bounded by one or more traits, `<T: Shape>` or `<T: Shape + Resizable>`. Inside the body the bounded parameter gets the trait's methods, and at each call the concrete type argument must implement the trait:

```neuro
func scaled_area<T: Shape>(s: &T, factor: i32) -> i32 {
    s.area() * factor          // dispatched through the `Shape` bound
}

// error: type argument `NoImpl` for 'T' does not implement required trait 'Shape'
```

A bound is enforced at the call site, and the body may call only the methods its bounds name.

### Copy and Clone bounds

`T: Copy` makes `T` copy instead of move inside the body, so a `T` may be read twice and a `Vec<T>` may be built there. `T: Clone` gives the body `v.clone()`, which returns a `T`, and a `Copy` bound gives it too. The scalars, `string` (which is `Clone` but not `Copy`), and a struct that derives or implements the trait all meet these bounds.

A shared reference meets both, whatever it refers to, and `.clone()` on a value declared `T` copies the reference. A receiver reads through its borrows only as far as the type parameter, so where `T` is `&Record`, `v.clone()` on a `v: T` never calls `Record`'s `clone`, while `s.clone()` on an `s: &Record` does. A `&mut T` meets neither bound: passing `&mut i32` for a `T: Copy` parameter is rejected with ``type argument `&mut i32` for 'T' does not implement required trait 'Copy'``. A value a generic call builds from a reference holds that borrow, a `Vec<T>` included.

```neuro
func dup<T: Copy>(v: T) -> (T, T) {
    val a = v
    val b = v        // fine: every instance copies
    (a, b)
}

func twice<T: Clone>(v: &T) -> T {
    v.clone()
}
```

### Where clauses

A constraint may move into a `where` clause after the return type. A `where` clause carries trait bounds (`where T: Shape`, enforced exactly as an inline bound) and **value predicates** over const parameters: a boolean expression checked at every instantiation and reported at the offending call.

```neuro
func head<const N: u32>(a: [i32; N]) -> i32 where N > 0 {
    a[0]           // guaranteed non-empty by `where N > 0`
}

func main() -> i32 {
    val xs: [i32; 3] = [10, 20, 12]
    return head(xs)   // 10
}
```

Calling `head` with a `[i32; 0]` array is rejected with ``the `where` predicate is not satisfied for this instantiation``.

## Associated Types

A trait may declare an **associated type**: a member type that each implementor chooses. The trait writes the name alone, the impl binds it, and either side names it as `Self::Name`:

```neuro
trait Channel {
    type Sample

    func sample(&self) -> Self::Sample
}

struct Tally {
    counted: i32
}

impl Channel for Tally {
    type Sample = i32

    func sample(&self) -> Self::Sample { self.counted }
}
```

`Self::Sample` resolves to what the impl bound, in a signature, in a method body and nested inside another type (`Option<Self::Sample>`). An impl may spell the position either way, so `-> i32` above means the same thing. Every declared associated type must be bound: ``impl Source for Counter does not bind associated type 'Item': add `type Item = <type>` ``.

### Constraining an associated type in a bound

A bare `T: Channel` erases the implementor, and with it the only thing that says what `Sample` is, so a generic body cannot call a method whose signature names it. The `Trait<Assoc = T>` form puts the answer in the bound:

```neuro
func scaled<T: Channel<Sample = i32>>(source: &T, factor: i32) -> i32 {
    source.sample() * factor       // typed as i32 by the bound
}
```

The same constraint is written wherever a bound is, in a `where` clause, in an argument-position `impl Trait`, and in return position, where it is a promise to the caller:

```neuro
func clause<T>(source: &T) -> i32 where T: Channel<Sample = i32> { source.sample() }

func anonymous(source: &impl Channel<Sample = i32>) -> i32 { source.sample() }

func make(seed: i32) -> impl Channel<Sample = i32> { Tally { counted: seed } }
```

At the call site, the concrete type's own binding must match the one the bound demands. Passing a `Channel` implementor whose `type Sample = f64` to a `Sample = i32` bound is rejected with ``bound `Channel<Sample = i32>` is not satisfied by Th, which binds `Sample` to f64``. A bare bound cannot type a call to a method that names the associated type:

```neuro
func unbound<T: Channel>(source: &T) -> i32 {
    source.sample()   // error: method 'sample' of trait 'Channel' names associated type `Self::Sample`, which this bound leaves open: write `Channel<Sample = T>` to say what it is
}
```

## impl Trait and dyn Trait

A trait bound can be satisfied two ways, and the keyword chooses which.

### `impl Trait`: static dispatch

`impl Trait` in argument position is anonymous-generic sugar. Each concrete type that flows through it produces a specialized copy of the function, as a named type parameter does, so there is no indirection. These two signatures compile to the same code:

```neuro
func train(model: &impl Model) -> i32 { model.step() }
func train_named<T: Model>(model: &T) -> i32 { model.step() }
```

Each `impl Trait` parameter is its own anonymous parameter, so one call may bind two different concrete types, unlike a single shared `<T>`:

```neuro
func total(a: &impl Shape, b: &impl Shape) -> i32 { a.area() + b.area() }
```

In **return** position, `impl Trait` names the one concrete type the body produces. The compiler resolves it to that type and verifies that the type implements the trait:

```neuro
func make() -> impl Shape { Square { side: 3 } }
```

The body must return a direct constructor (a struct literal or enum value) for the concrete type to be inferable. Other forms, such as a binding, are rejected with ``cannot infer the concrete type of the `impl Shape` return``.

### `dyn Trait`: dynamic dispatch

`&dyn Trait` is a single runtime type that can hold any implementor. The reference carries a pointer to the value and a pointer to that type's method table, and a method call jumps through a fixed slot in the table. One function body therefore serves values of different concrete types:

```neuro
func measure(s: &dyn Shape) -> i32 { s.area() }

// measure(&square) and measure(&rect) both coerce to &dyn Shape.
```

A trait object is unsized, so it appears only behind a reference, `&dyn Trait` or `&mut dyn Trait`. A bare `dyn Shape` is rejected with ``` `dyn Shape` is unsized and must appear behind a reference ```. A `&mut self` method requires a `&mut dyn Trait` receiver, and a call through a `&dyn` receiver is rejected.

A trait object reaches its supertraits' methods as well: the table behind a `&dyn Priced` carries `Named`'s methods after `Priced`'s own, so `item.name()` works. A `&dyn Priced` does not coerce to a `&dyn Named`.

A derived or builtin implementor coerces too: a struct with `@derive(Debug)` becomes a `&dyn Debug`, and so does `&n` for an `i32`. A `string` coerces when the borrow names the place holding it (`&s`, `&p.name`). A `&string` parameter or a view has no such place, so it is refused.

A default method reached through `dyn` runs the trait's body, and an override runs the implementor's.

| Form         | Dispatch | Cost                   | Use when                                       |
| ------------ | -------- | ---------------------- | ---------------------------------------------- |
| `impl Trait` | static   | zero (monomorphized)   | one concrete type per call site                |
| `&dyn Trait` | dynamic  | one vtable indirection | a heterogeneous set behind a uniform interface |

Named bounds (`<T: Shape>`) dispatch statically, like `impl Trait`. Use `dyn` when values of different concrete types must be handled behind one interface, and `impl Trait` or a named bound when each call site has one concrete type.

## Object Safety

A trait is usable as `dyn` only if it is **object-safe**: every method must take a `&self` or `&mut self` receiver, name `Self` nowhere else and declare no type parameters of its own, so the vtable has a fixed layout. A method that consumes `self` by value, a method with no receiver, a method that names `Self` outside its receiver, and a generic method each make the trait unusable as a trait object. Such a trait still works through `impl Trait` and named bounds.

```neuro
trait Consume { func take(self) -> i32 }        // consumes self
trait Maker   { func build() -> i32 }           // no receiver
trait Grow    { func grown(&self) -> Self }     // names Self outside its receiver
trait Echo    { func echo<U>(&self, u: U) -> U } // one instance per U
```

Using one of them as a trait object is an error that names the rule, for example ``trait 'Consume' is not object-safe and cannot be used as `dyn Consume`: method 'take' takes `self` by value``.

A trait is object-safe only if its supertraits are, since their methods share the vtable. `Clone` names `Self` in its return type, so neither `dyn Clone` nor `dyn Copy` is accepted, nor a `dyn` of any trait requiring them. Operator traits fail the same test, whereas `Debug` and `Hashable` pass it.

A trait that declares an associated type is rejected as `dyn` as well. A trait object would have to bind the type (`&dyn Channel<Sample = i32>`), and that form is not implemented.

## See also

- [Functions](functions.md): closures and function values.
- [Structs](structs.md): generic enums, `@derive`, and the derivable traits.
- [Operators](operators.md#operator-overloading): the operator traits.
- [Ownership](ownership.md): moves, `Drop`, and element ownership.
- Examples: [`structs/generic_methods.nr`](../../examples/structs/generic_methods.nr), [`structs/supertraits.nr`](../../examples/structs/supertraits.nr), [`types/assoc_bounds.nr`](../../examples/types/assoc_bounds.nr), [`types/dispatch.nr`](../../examples/types/dispatch.nr), [`showcase/shape_traits.nr`](../../examples/showcase/shape_traits.nr), [`showcase/typed_channels.nr`](../../examples/showcase/typed_channels.nr), [`showcase/generic_toolkit.nr`](../../examples/showcase/generic_toolkit.nr).
