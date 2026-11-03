# Ownership and Borrowing

Neuro has no garbage collector and no reference counting. Each non-`Copy` value has one owner, a move hands it to a new owner, and a borrow lends it without taking it, which is how the compiler knows when each value is released. Read this page when a program fails with a `use of moved value`, `cannot borrow` or `cannot move out` error, or before writing a function that takes or returns a reference. For what is released and when, see the [memory model](memory-model.md).

## Moves

For **non-`Copy`** types (`string`, the collections, structs without `@derive(Copy)`), handing the value to a new owner *moves* it. The source binding becomes invalid, and reading it is a compile error:

```neuro
val s1: string = "Hello"
val s2: string = s1        // s1 is MOVED into s2
val n: u64 = s1.len()      // error: use of moved value 's1'
val m: u64 = s2.len()      // OK, s2 owns the value now
```

A move happens wherever a non-`Copy` value is handed to a new owner: a `val` or `mut` initializer, an assignment, a `return` or a function's last expression, a struct field store, a by-value call argument, the left side of `??`, the operand of `?`, or a `match` arm that binds an owning payload by value. An `if`, `match` or block in one of those positions moves whatever its arms yield.

```neuro
func consume(s: string) -> u64 { s.len() }

func main() -> i32 {
    val greeting: string = "Hi"
    val len: u64 = consume(greeting)   // greeting is moved into consume()
    val again: u64 = greeting.len()    // error: use of moved value 'greeting'
    return len as i32
}
```

A value reached through a borrow owns nothing to give away, so moving it is an error. Moving a field out of a `&self` receiver fails with `cannot move out of 'self'`. Bind a `.clone()` instead. A moved `mut` binding is valid again once it is assigned a new value.

### Moves in branches

A move inside one arm of an `if` or `match` does not affect the other arms. Past the branch the binding may already be gone, so reading it there is a use of a moved value. An arm that leaves (`return`, `break`, `continue` or `panic`) is the exception, because no path past the branch runs it:

```neuro
func consume(s: string) -> u64 { s.len() }

func pick(ready: bool) -> u64 {
    val msg: string = "hi"
    if ready {
        val r: u64 = consume(msg)   // moves msg on this path
    }
    val n: u64 = msg.len()          // error: use of moved value 'msg'

    val other: string = "hey"
    if !ready {
        return consume(other)       // moves other, but this path leaves
    }
    return other.len()              // OK: every path that reaches here kept `other`
}
```

### Moves in loops

A loop body may not move a binding declared outside the loop, because the next iteration would move it again. The diagnostic is `'s' is moved out inside a loop body, but it is bound outside the loop`. Give the binding a new value before the iteration ends, as `mut` allows, or move a `.clone()` instead:

```neuro
func consume(s: string) -> u64 { s.len() }

func main() -> i32 {
    mut s: string = "ab"
    mut total: u64 = 0
    for i in 0..3 {
        total = total + consume(s)   // moves s, which the next pass needs
        s = "cd"                     // OK: s is owned again before the pass ends
    }
    return total as i32
}
```

Without the `s = "cd"` line, the loop is refused with the diagnostic above.

## Copy and clone

Integers, `f32`, `f64`, `bool` and `char` are `Copy`. Assigning one duplicates it, and the source stays valid:

```neuro
val a: i32 = 5
val b: i32 = a             // a is COPIED
val c: i32 = a + b         // both a and b still valid
```

Aggregates built only from `Copy` parts, references, and `@derive(Copy)` structs are `Copy` too. A type that implements `Drop` cannot be `Copy`, so `@derive(Copy)` on it is an error (`type 'G' implements Drop and so cannot be Copy`).

`.clone()` is the opt-out. It borrows its receiver, so the original stays valid:

```neuro
val a: string = "hello"
val b: string = a.clone()  // a is NOT moved
val ok: bool = a == b      // reading a here is fine
```

`@derive(Clone)` refuses a struct with a field that owns a buffer: a `string`, a tensor, a `Vec` or a `StringBuilder`. For a `string` or tensor field, write `impl Clone` by hand and call `.clone()` on the field. A `Vec` and a `StringBuilder` have no `.clone()` to call, so a struct holding one cannot implement `Clone` yet.

## Ownership of elements

An aggregate that holds an owner is an owner. Arrays, tuples, structs, enum payloads, newtypes and generic instances follow the same rules when what they hold is not `Copy`.

**The aggregate moves.** Passing or binding it hands over everything inside, so the source binding is spent:

```neuro
val pair: (string, string) = ("left", "right")
val taken = pair        // moves both elements
println(pair.0)         // error: use of moved value 'pair'
```

**One element at a time.** Reading an element out moves that element and leaves its siblings owned. Destructuring works the same way:

```neuro
struct Two { a: string, b: string }

func main() -> i32 {
    val t = Two { a: "x", b: "y" }
    val first = t.a         // moves `t.a`
    val second = t.b        // `t.b` is untouched and still owned
    val Two { a, b } = Two { a: "p", b: "q" }   // each field moves into its own binding
    return (first.len() + second.len() + a.len() + b.len()) as i32
}
```

**A partially moved value cannot be read whole.** Once any element has left, naming the aggregate as a whole is an error:

```neuro
struct Two { a: string, b: string }
func consume(t: Two) -> u64 { t.a.len() + t.b.len() }

func main() -> i32 {
    val t = Two { a: "x", b: "y" }
    val first = t.a
    consume(t)              // error: use of moved value 't'
    return first.len() as i32
}
```

**A store puts an element back.** Assigning to a moved-out place refills it. The aggregate is whole again once every place that left has been refilled:

```neuro
struct Two { a: string, b: string }
func consume(t: Two) -> u64 { t.a.len() + t.b.len() }

func main() -> i32 {
    mut t = Two { a: "x", b: "y" }
    val first = t.a
    t.a = "z"               // `t.a` is owned again
    return consume(t) as i32   // fine: nothing is missing
}
```

A place refilled on only some paths (one arm of an `if` without an `else`) may still be moved where the paths meet. A place whose aggregate, or an enclosing field, left as a whole has nothing to refill, so assign the aggregate instead.

**An index names an element only when it is a literal.** `a[0]` moves element `0`. An index the compiler cannot evaluate, such as `a[i]` with a runtime `i`, names no particular element, so it moves the whole binding `a`. Use `.clone()` on the element when the aggregate has to stay intact.

**A `Vec` slot cannot be left empty.** Reading a non-`Copy` element out of a `Vec` by value is refused with ``cannot move a `string` out of a `Vec` slot``. Borrow it with `&v[i]`, copy it with `v[i].clone()`, or take it out with `v.remove(i)`.

## Borrows (`&T`)

An **immutable borrow** `&T` is a non-owning reference. A function reads a value through it without taking ownership, so the caller keeps its binding. The borrow expression `&x` takes a reference to the place `x`:

```neuro
func describe(s: &string) -> u64 {
    s.len()                 // method call auto-derefs through the borrow
}

func main() -> i32 {
    val msg: string = "Neuro"
    val n: u64 = describe(&msg)   // borrow, msg is NOT moved
    val again: u64 = msg.len()    // still valid: borrowing never consumes
    return 0
}
```

- A reference type `&T` may appear on parameters, return types, and local bindings.
- `&T` is itself `Copy`: passing or re-borrowing a reference duplicates the pointer.
- Method calls and field reads auto-deref through any number of `&` and `&mut`. `r.len()` works on a `&string`, `r.method()` on a `&Struct` or a `&&Struct`, `r.abs()` on a `&f64`, and `r.field` on a `&Struct`. A field write goes through one `&mut`.
- A borrow of a borrow is written `&&T` in a type. In an expression, borrow a binding that holds the reference (`val rr = &r`), because `&&x` does not borrow a place.
- Only a **place** can be borrowed: a `val`, `mut` or parameter binding, or a field, a tuple element, or an array, slice or `Vec` element reached from one (`&p.name`, `&arr[i]`, `&v[i]`). A borrow of a part borrows the whole binding, so `p` may not be moved while `&p.name` is live. A `const` and a tensor element are not places, and `&` on them is refused with ``cannot borrow this expression: `&` requires a place``.
- A **temporary** (a literal, an operator or call result, a struct literal) lives until the end of its statement. A shared borrow of a `Copy` temporary can therefore be passed straight to a call: `id(&1)`, or `norm(&Point { x: 3, y: 4 })` when `Point` is `@derive(Copy)`. A borrow of a temporary cannot be bound (`val r = &5` is refused). A call that could keep the borrow, in its result or through a `&mut` argument, refuses it too.

Not yet supported: borrowing an owning temporary, such as `&make_string()`. Bind the value to a `val` first.

### String slices (`&string`)

`&string` is also the borrowed string slice: a non-owning view of UTF-8 data. There is no separate slice type, so a `&string` is both a borrow of an owned `string` and a slice of one. Equality is the operation a slice needs most. `==` and `!=` compare the bytes for any mix of `string` and `&string`, so comparing through a borrow neither moves nor copies:

```neuro
val lang: string = "Neuro"
val same: bool = (&lang == "Neuro")   // slice vs owned literal, lang stays usable
```

`string` slicing methods and the two string representations are covered in [Strings](strings.md).

## Mutable borrows (`&mut T`)

A **mutable borrow** `&mut T` is a non-owning reference that grants **write** access. The borrow expression `&mut x` requires `x` to be a `mut` binding. Values are read and written through the prefix **dereference operator** `*`:

```neuro
func increment(n: &mut i32) {
    *n = *n + 1          // read with *n, write with *n = ...
}

func main() -> i32 {
    mut counter: i32 = 40
    increment(&mut counter)       // mutate in place, counter is borrowed, not moved
    increment(&mut counter)
    return counter                // 42
}
```

**Rules:**

- `&mut x` requires a `mut` binding. Mutably borrowing a `val` fails with `cannot mutably borrow 'x'`.
- `*r` reads the referent. Dereferencing a non-reference fails with `cannot dereference`.
- `*r = value` requires `r: &mut T`. Writing through an immutable `&T` fails with `cannot assign through an immutable reference`.
- A `&mut T` converts to `&T` only as a call argument, where a `&T` parameter takes a shared reborrow. Anywhere else the types must match: `val r: &i32 = &mut x` fails with `type mismatch: expected &i32, found &mut i32`.

```neuro
func main() -> i32 {
    mut x: i32 = 7
    val r: &mut i32 = &mut x
    *r = 35
    return *r                     // 35
}
```

### Reading through a borrow

Arithmetic, comparison, bitwise and logical operators accept a borrowed operand wherever they accept an owned one. The same holds for `+` and `==` on strings, tensor operators, compound assignment and string interpolation. The result is what the owned operands would give, and the operand stays borrowed. A bare literal next to a borrowed operand takes the referent's type:

```neuro
func clamp_add(x: &i64, step: &mut i64) -> i64 {
    if x > 100 { return x - 1 }
    *step += 1
    2 * x + step.saturating_mul(2)   // i64 arithmetic; x and step are only read
}
```

Anywhere else a borrow keeps its own type. `val y = *x` copies the referent out, `val y = x` copies the borrow, `x as i64` is an error, and a write still goes through `*r = v`.

## Borrow exclusivity

Two rules govern borrows of one place, and a third governs the **borrowee**, the place a borrow points at:

- **Any number of shared `&T` borrows** of a place may be live at the same time.
- **A `&mut T` borrow is exclusive**: while it is live, no other borrow of that place, shared or mutable, may exist.
- While a `&mut T` of a place is live, the place may not be **read**, **written**, or **moved** through its own name. Every access goes through the borrow.
- While *any* borrow of a place is live, the value may not be **moved out** of it, nor the binding **assigned** a new value. Either would leave the borrow pointing at storage the binding no longer owns.

A borrow's region is **lexical**. A borrow held by a binding (`val r = &x`) lives until that binding leaves scope. A borrow passed to a function, used in a condition, or returned ends with the statement that took it. Sequential borrows in separate statements never conflict, and a borrow taken inside a block is released at the block's closing brace:

```neuro
func main() -> i32 {
    mut x: i32 = 5
    val a: &i32 = &x
    val b: &i32 = &x          // ok, shared borrows coexist
    val c: &mut i32 = &mut x  // error: cannot borrow 'x' as mutable: it is already borrowed
    return 0
}
```

```neuro
func inc(n: &mut i32) { *n = *n + 1 }

func main() -> i32 {
    mut x: i32 = 0
    inc(&mut x)               // the &mut ends with this call
    inc(&mut x)               // ok, the previous borrow is no longer live
    return x                  // 2
}
```

```neuro
func consume(s: string) -> u64 { s.len() }

func main() -> i32 {
    mut n: i32 = 1
    val r: &mut i32 = &mut n
    val read: i32 = n         // error: cannot use 'n' while it is mutably borrowed
    *r = 5                    // write through the borrow instead

    val s: string = "hello"
    val b: &string = &s
    val len: u64 = consume(s) // error: cannot move out of 's' while it is borrowed
    return 0
}
```

The diagnostics are:

| Message starts with | Cause |
|---|---|
| `cannot borrow 'x' as mutable` | A `&mut` while any other borrow is live |
| `cannot borrow 'x' as immutable` | A `&` while a `&mut` is live |
| `cannot use 'x' while it is mutably borrowed` | Any access through the frozen name |
| `cannot move out of 'x' while it is borrowed` | A move of the borrowee |
| `cannot assign to 'x' while it is borrowed` | An assignment to the borrowee |

> **By design:** the borrow region is **lexical**, not non-lexical liveness (NLL). A borrow held by a binding freezes its borrowee until that binding leaves scope, even when the borrow is never used again. Read back through the borrow, or confine the borrow to a block, so the block separates a shared read from a later `&mut`.

## Lifetimes and returned references

Lifetimes are **inferred** in most code. A function that returns a reference may borrow one of its reference parameters, and the caller's borrow then lasts as long as the result's binding. With one reference parameter, the result borrows that parameter. With several and no `self`, the result may borrow any of them, and the caller's borrow of each one lasts as long as the result. Methods with a `&self` receiver follow the same rule, with the receiver among the candidates. This differs from Rust, which asks for explicit lifetimes when a function has several reference inputs:

```neuro
func first(a: &i32, b: &i32) -> &i32 {
    a                       // ok, the returned borrow outlives the call
}
```

The borrow checker rejects returning a reference to a value that dies when the function returns, such as a body-local or a by-value parameter:

```neuro
func dangle() -> &i32 {
    val local: i32 = 5
    return &local           // error: cannot return a reference to 'local'
}
```

The check follows a returned reference through a local reference binding (`val r = &local; r` is also rejected), through a value holding one (see [References held in values](#references-held-in-values)), and into the arms of a returned `if` or `match`.

### Explicit lifetime annotations

For patterns that elision cannot express, declare a lifetime parameter in the generic-parameter list as `'a` and use it on reference types as `&'a T` or `&'a mut T`:

```neuro
func longest<'a>(a: &'a string, b: &'a string) -> &'a string {
    if a.len() > b.len() { a } else { b }
}
```

Every lifetime used on a reference must be declared in the enclosing `<...>` list (an undeclared `'b` is an `undeclared lifetime` error). The annotation is then **erased**, so `&'a T` and `&T` are the same type. Explicit lifetimes never change which values a signature accepts. Lifetime parameters may be mixed with type and const parameters (`func f<'a, T>(...)`). Lifetime bounds (`'a: 'b`) are not supported.

### References held in values

A struct field, an enum or `Option` payload, and a tuple or array element may hold a reference. The struct or enum declares lifetime parameters for it, and so may an `impl` block for the type:

```neuro
struct View<'a> { text: &'a string, from: u64 }
enum Pick<'a, T> { Borrowed(&'a T), Owned(T) }

impl<'a> View<'a> {
    func source(&self) -> &'a string { self.text }
}

func view<'a>(text: &'a string, from: u64) -> View<'a> {
    View { text: text, from: from }
}
```

A use of the type names its lifetimes before its type arguments (`View<'a>`, `Pick<'a, i32>`), writes `'_`, or leaves them out.

The binding that holds such a value holds its borrows, just as `val r = &s` holds one. While it lives, the borrowee cannot be moved, assigned or borrowed `&mut`. The value takes the borrows written into it, the borrows of the holder it was copied from or is part of (including a `match` or `for` binding), and, for a call's result, the borrows of every argument and the receiver, since under elision the result may hold any of them. A holder keeps its borrows until its own scope ends, even after its value has moved on, so two holders of `&mut x` need separate blocks.

Three stores are refused:

| Store | Diagnostic |
|---|---|
| A borrow put into a binding declared outside the borrowee's scope: `r = &s`, `h = H { r: &s }`, `h.r = &s`, or `set(&mut h, &s)` from an inner block | `'s' does not live long enough` |
| A value holding a borrow of a local, returned | `cannot return a reference to 's'` |
| The same value stored through a `&mut` parameter or `self` | `cannot store a borrow of 's' through a reference` |

A call is judged conservatively: a signature does not say what the callee keeps, so a `&mut` argument whose type can hold a reference takes every borrow the call's other arguments carry. [`borrowed_records.nr`](../../examples/showcase/borrowed_records.nr) shows these rules working together.

## Destructors (`impl Drop`)

A struct can define a destructor by implementing the built-in `Drop` trait. Its `drop(&mut self)` method runs automatically when an owner of the value goes out of scope:

```neuro
struct Guard {
    sink: &mut i32
}

struct Slot {
    guard: Guard
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
    }                 // `g` goes out of scope here, so `drop` runs: dropped == 1
    mut in_slot: i32 = 0
    {
        val held = Slot { guard: Guard { sink: &mut in_slot } }
    }                 // the field goes with the struct that holds it
    return dropped + in_slot    // 2
}
```

**Rules:**

- Destructors run **only on normal scope exit**: fall-through, `return`, `break`, and `continue`. A panic aborts the process without running any destructor, because there is no stack unwinding.
- When several owned values leave the same scope, they are dropped in **reverse declaration order** (LIFO).
- A value moved out (rebound, returned, passed by value, or stored into a struct) is dropped exactly once, by its final owner.
- Only a struct may implement `Drop`. An enum that does is refused: ``an enum may not implement `Drop`; only a struct may``.
- The `drop` method must be exactly `drop(&mut self)`: no extra parameters and no return type. An `impl Drop` block may contain no other methods.
- Reassigning a `mut` binding runs the displaced value's destructor at the assignment. The new value is built first, so the reassignment may read the value it replaces. A value already moved out is not destroyed a second time.
- A holder owns what it holds. When a value is destroyed, so is every owner inside it: a struct field, an array or tuple element, an enum payload, and a newtype's inner value. The holder's own `drop` runs first, then its fields in declaration order, and each position is destroyed exactly once.
- A position given up first is destroyed on its own path instead. Moving `p.field` into another binding leaves its siblings owned by `p`, and assigning to `p.field` destroys the value it displaces there.
- Inside a `pool` block, a type with a destructor must implement `PoolAware` to be owned there. See [Pool blocks](control-flow.md#pool-blocks).

Not yet supported: an enum's `string` payload is never released, so an enum value holding a `string` leaks it. Other owners in a payload, such as a `Vec` or a `Drop` struct, are released with the enum. See [What still leaks](memory-model.md#what-still-leaks).

## See also

- [Memory model](memory-model.md): what is released and when, and what still leaks
- [Control flow](control-flow.md#pool-blocks): `pool` arena blocks and the values they may hold
- [Strings](strings.md): string representations, slicing and moves of `string` values
- [Structs](structs.md): `impl Drop`, `@derive` and struct lifetimes
- [Variables](variables.md): bindings, reassignment and `mut`
- [Ownership examples](../../examples/ownership/): runnable programs for each rule above
