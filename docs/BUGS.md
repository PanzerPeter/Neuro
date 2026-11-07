# Known Bugs

Open defects only, newest first. Every confirmed bug that is not yet fixed has a numbered
`BUG-NNN` entry here; when a bug is fixed its entry is **deleted**, the fix lives in
`CHANGELOG.md`, in the affected slice's `CONTEXT.md`, and in its regression test. IDs are
never reused, so numbering stays stable as entries are removed.

## BUG-144: a discarded call result that passes a parameter through is never released

- **Status**: open, confirmed
- **Area**: `llvm-backend` (drops of expression statements)
- **Severity**: minor. A leak, one value per discarded call; nothing is read out of bounds

**Minimal repro**

```neuro
struct M { v: Vec<i32> }

func make() -> M {
    mut v: Vec<i32> = Vec::new()
    v.push(1)
    M { v: v }
}

func same(m: M) -> M { m }

func main() -> i32 {
    same(make())
    0
}
```

Expected: the `Vec`'s buffer is released at the end of the statement.
Observed: LeakSanitizer reports it. `-make()` leaks the same way with `func neg(self) -> M { self }`,
and a call that builds a new value (`make() + make()`) does not leak.

**Root cause**: not confirmed. Found while testing operators on non-`Copy` types.

## BUG-143: the gradient of a device parameter can land on the host

- **Status**: open, confirmed
- **Area**: `hir-lowering` (`autodiff`, the reverse of `.sum()` and `.mean()`)
- **Severity**: minor. The values are right; an update pays a host-to-device copy it should not need

**Minimal repro**

```neuro
@grad(wrt: [v])
func loss(v: &mut Tensor<f32, [2, 1]>) -> Tensor<f32, []> {
    return Tensor::scalar(v.sum())
}

export func gradient() -> Tensor<f32, [2, 1]> {
    val h: Tensor<f32, [2, 1]> = [[1.0], [2.0]]
    mut w = h.to(Device::GPU(0))
    val e = loss(&mut w)
    e.backward()
    return w.grad().clone()
}
```

Link it into a shared library and read `dl_tensor.device` from the returned handle.

Expected: `kDLCUDA`, the parameter's device. Observed: `kDLCPU`. `.mean()` directly on the parameter
does the same. Squaring the parameter first, `.tanh()` and `x @ w - y` all leave the gradient on the
GPU, so where a gradient lives depends on the last operation applied to the parameter. A training
step still computes the right update, because a host operand next to a device operand is copied over.

**Root cause**: confirmed for `.sum()`. Its reverse builds the broadcast adjoint as a host tensor
literal, visible in the IR of `__loss__rev`, and that value becomes the gradient unchanged.

**Fix sketch**: move a gradient to its parameter's device where `.backward()` stores it, which covers
every rule at once. Regression tests: the repro through the DLPack device field, and `.mean()`.

## BUG-142: `val ... else` over a call result never releases an owning payload

- **Status**: open, confirmed
- **Area**: `llvm-backend` (`codegen_val_else`, `ArmOwnership::EnclosingString` in
  `codegen/expressions/matches.rs`)
- **Severity**: major. An unbounded leak, one payload per evaluation

**Minimal repro**

```neuro
func make() -> Option<Tensor<f32, [2]>> {
    val t: Tensor<f32, [2]> = [1.0, 2.0]
    return Some(t)
}

func main() -> i32 {
    mut total = 0.0f32
    for i in 0..10 {
        val Some(c) = make() else { return 2 }
        total += c.sum()
    }
    return 0
}
```

Expected: `c` is released at the end of each iteration. Observed: AddressSanitizer's leak check
reports the tensor's allocations for the earlier iterations. A `Vec` payload leaks the same way, and
so does `val Ok(c) = load() else |e| { ... }`.

`if val`, `match`, `?` and `??` on the same call are clean, and so is binding the call first
(`val r = make()`, then `val Some(c) = r else { ... }`).

**Root cause**: confirmed in the code. `codegen_val_else` binds its success bindings with
`ArmOwnership::EnclosingString`, which registers a `string` payload from a collection's fallible
reader and leaves every other payload unregistered. A named scrutinee still owns its payload. A call
result has no binding that does, so nothing releases it.

**Workaround**: bind the call to a `val` first.

**Fix sketch**: when the scrutinee is a temporary, register a payload that `holds_owner` in the
enclosing scope, where the binding lives. Check the `else` branch too, whose scrutinee is equally
unowned. Regression tests: the repro with a tensor and a `Vec`, a miss that runs the `else` branch,
and a named scrutinee (must not double free).

## BUG-140: `Device`'s variants are bare names in every module

- **Status**: open, confirmed
- **Area**: `neurc` (`prelude.rs`, `Prelude::variants`)
- **Severity**: minor. A program that should need `Device::CPU` compiles; nothing is read out of
  bounds

**Minimal repro**

```neuro
func main() -> i32 {
    val t: Tensor<f32, [2]> = [1.0, 2.0]
    val moved = t.to(CPU)
    0
}
```

Expected: `CPU` is undefined. The prelude names `Device` but not its variants, so only `Option`'s
and `Result`'s are bare, as [Modules](language-reference/modules.md#the-implicit-prelude) says.
Observed: the program compiles.

**Root cause**: confirmed in the code. `load` binds the variants of every prelude enum except those
in `QUALIFIED_ENUMS`.

**Workaround**: write `Device::CPU`.

**Fix sketch**: add `Device` to `QUALIFIED_ENUMS`, or invert the list to name `Option` and `Result`.
Any program using a bare `CPU` or `GPU(0)` then needs the prefix. Regression test: the repro is
refused, and `Device::CPU` still works.

## BUG-139: the unit type `()` cannot be written in a type position

- **Status**: open, confirmed
- **Area**: `syntax-parsing` (`parse_type`); `semantic-analysis` (`UnsupportedEnumPayload`)
- **Severity**: minor. A valid program is refused with a clear diagnostic

**Minimal repro**

```neuro
func f(o: Result<(), i32>) -> i32 {
    match o {
        Result::Ok(_) => 1,
        Result::Err(e) => e
    }
}

func main() -> i32 { 0 }
```

Expected: this compiles, since `()` is the zero-element tuple. Observed: "unexpected token
RightParen, expected a tuple type `(T1, T2, ...)` or a function type `(T1, ...) -> R`".

**Root cause**: confirmed in the code. `parse_type` reads `()` only as the start of a function
type, so it refuses one with no `->` after it.

**Workaround**: use a placeholder integer as the success payload.

**Fix sketch**: parse `()` as the unit type. The next stop is likely `UnsupportedEnumPayload`,
which refuses a `void` payload, so a unit payload also needs a layout: a zero-word slot, read by no
binding. Regression tests: the repro, `Ok(())` as a value, and a `()` return annotation.

## BUG-138: a private function is reachable by its bare name after `import`

- **Status**: open, confirmed
- **Area**: `module-resolution` (the flat merge); `semantic-analysis` (call resolution)
- **Severity**: minor. A program that breaks the visibility rule compiles; nothing is read
  out of bounds

**Minimal repro**

Two files side by side:

```text
// lib.nr
export func shown() -> i32 { 1 }

func internal() -> i32 { 4 }

// main.nr
import ./lib

func main() -> i32 {
    internal()
}
```

Expected: an error at `internal()`, since `internal` is not exported. Observed: `neurc run
main.nr` exits 4. The qualified `lib::internal()` is refused as it should be.

**Root cause**: confirmed in the behaviour, with the mechanism known. Modules merge into one flat
namespace, and visibility is checked only where a
path or an import list names the item. A bare name resolves in the merged namespace, where every
module's private items sit beside the root's.

**Workaround**: none needed to compile; to keep the rule, call imported items qualified or
through an import list, which are checked.

**Fix sketch**: record each merged item's home module and refuse a bare reference to a private
item from another module. The hierarchical module namespaces planned for the standard library
remove the flat merge and this with it. Regression tests: a bare call, a bare type name and a
bare constant from a private item, each refused, and the same names from inside their own module
still accepted.

## BUG-132: a borrow of an owning temporary is refused

- **Status**: open, confirmed. Narrowed: a shared borrow of a `Copy` temporary passed to a
  call (`id(&1)`, `norm(&Point { x: 3, y: 4 })`) now compiles
- **Area**: `semantic-analysis` (`check_temporary_borrow`); `llvm-backend` (`codegen_reference`)
- **Severity**: minor. A valid program is refused with a clear diagnostic and a workaround

**Minimal repro**

```neuro
func size(s: &string) -> i32 { s.len() as i32 }

func main() -> i32 {
    val k: string = "x"
    size(&(k + "yz"))
}
```

Expected: exit 3. The operand of `&` may be a temporary that lives until the end of its
statement, so its borrow may be passed to a call. Observed: "cannot borrow this expression: `&`
requires a place".

**Root cause**: confirmed in the code. `check_temporary_borrow` accepts only a `Copy` operand,
because an owning temporary (`&make_string()`, `&build_vec()`) must be released once the call
returns, and nothing emits that release: `codegen_reference` stores the value into a stack slot
and lends its address, with no drop entry for the slot.

**Workaround**: bind the value to a `val` first and borrow the binding.

**Fix sketch**: register the slot `codegen_reference` fills as an owner released at the end of
the statement (the statement-end release the backend runs for anonymous strings, extended to the
other owning types), and drop the `Copy` restriction. A `&string` is passed by value, so its
temporary needs the same release with no slot. Regression tests: the repro under ASan, a `Vec`
and a `Drop` temporary, and `val r = &f()` still refused.

## BUG-130: a reference into `self` returned from a `&self` method dangles

- **Status**: open, confirmed
- **Area**: `llvm-backend`; the `&self` receiver ABI of struct methods
- **Severity**: major. A silent use after return: the program compiles and reads a dead frame

**Minimal repro**

```neuro
struct Bag { xs: [i32; 3] }
struct View<'a> { xs: &'a [i32; 3] }

impl Bag {
    func view(&self) -> View { View { xs: &self.xs } }
}

func noise(a: i32, b: i32) -> i32 {
    val arr = [a, b, a, b]
    arr[0] + arr[3]
}

func main() -> i32 {
    val b = Bag { xs: [1, 2, 3] }
    val v = b.view()
    val n = noise(70, 80)
    v.xs[1] * 10 + v.xs[2]
}
```

Expected: exit 23. Observed: the elements read back as whatever `noise` left on the stack.
An iterator built this way (`func iter(&self) -> BagIter { BagIter { xs: &self.xs, at: 0 } }`)
yields its first element and then garbage.

**Root cause**: confirmed in the emitted IR. `Bag__view` takes the struct by value
(`{ [3 x i32] } %0`), stores it into its own `alloca`, and `&self.xs` is the address of that
slot, which dies when the method returns. The checker accepts the program, because by the
language's rules the result borrows `b`.

**Workaround**: build the view at the call site (`View { xs: &b.xs }`), or take the field by
a parameter: `func view(xs: &[i32; 3]) -> View`.

**Fix sketch**: pass a `&self` / `&mut self` receiver as the address of the caller's place,
as a `&T` parameter already is, at least for a method whose result can hold a reference.
Regression tests: the repro, and the iterator form driven by a `for` loop.

## BUG-128: a method's reference into a local receiver can be returned

- **Status**: open, confirmed
- **Area**: `semantic-analysis`; `check_escaping_borrows` in `type_checkers/statements/returns.rs`
- **Severity**: major. A silent use after free: the program compiles and reads released memory

**Minimal repro**

```neuro
struct B { s: string }

impl B {
    func head(&self) -> &string { self.s.slice(0..1) }
}

func first() -> &string {
    val b = B { s: "x" + "y" }
    b.head()
}

func main() -> i32 {
    first().len() as i32
}
```

Expected: a compile error, as for `b.s.slice(0..1)` written as the tail. Observed: it compiles,
`b`'s buffer is released when `first` returns, and the caller reads it.

**Root cause**: confirmed in the code. The returned-value walk follows a method call's arguments
and what its receiver holds, but never the receiver itself. Under elision a method's returned
reference borrows `self`, so an owned local receiver is a local the result points into. The walk
cannot simply report it: a method whose result names the impl's lifetime
(`func source(&self) -> &'a string`) returns what the receiver borrows, not the receiver, and
lifetime arguments in a type application (`-> View<'a>`) are dropped by the parser, so the
checker cannot tell the two apart for every return type.

**Workaround**: none in the compiler; return the receiver itself, or a borrow of a parameter.

**Fix sketch**: record, per method, whether its result borrows `self` (an elided output lifetime)
or only the impl's lifetimes, which needs the lifetime arguments of a type application kept in
the AST. Then report an owned local receiver of a `self`-borrowing method in the returned-value
walk. Regression tests: the repro, and the same call on a method returning `&'a string` from a
field, which must keep compiling.

## BUG-126: a `&string` cannot become a `&dyn Debug` or `&dyn Hashable`

- **Status**: open, confirmed
- **Area**: `semantic-analysis` (the `&T` to `&dyn Trait` coercion) and `llvm-backend`
  (trait-object construction)
- **Severity**: minor. A program the language accepts is refused with a clear diagnostic

**Minimal repro**

```neuro
func render(d: &dyn Debug) {}
func main() -> i32 {
    val s = "hi"
    render(&s)
    0
}
```

Expected: the program builds, since `string` implements `Debug` and `Hashable` with no impl to
write, and a trait object holds any implementor. Observed: `type mismatch: expected &dyn Debug,
found &string`. A generic body that coerces `&T` with `T = string` fails in lowering with
`a &string cannot become a &dyn Debug`. Every other builtin and every derived implementor coerces.

**Root cause**: a `&string` is a `{ ptr, len }` fat pointer, and a trait object's data pointer is a
thin pointer to the value. `&s` of a `string` binding has a place holding the fat pointer (its
stack slot), but a `&string` parameter does not, and spilling one to the stack would dangle once
the function returns the trait object.

**Workaround**: wrap the text in a struct that derives `Debug` (or implement `Debug` for one) and
coerce a reference to that.

**Fix sketch**: give the coercion the address of the place that holds the fat pointer when the
operand is a borrow of a `string` binding, and keep refusing a `&string` value that has no place.

## BUG-124: a `for` over an array of `string` leaks every element

- **Status**: open, confirmed
- **Area**: `llvm-backend`, `codegen_for_each` in `codegen/expressions/arrays.rs` and the
  `string` ownership flags
- **Severity**: minor. One buffer leaks per element per loop; no value is wrong

**Minimal repro**

```neuro
func q() -> string { "a" + "b" }

func once() -> i32 {
    val parts = [q(), "x" + "y"]
    mut total = 0
    for part in parts {
        total += part.len() as i32
    }
    total
}

func main() -> i32 {
    mut t = 0
    for i in 0..50 {
        t += once()
    }
    t / 50
}
```

Expected: exit 4 and every buffer released. Observed: exit 4, and LeakSanitizer (`LD_PRELOAD`
of `libasan.so`) reports both buffers of every call, whatever the order of the elements. With
a single call the stack still holds the pointers, so the leak check may report only one of them.

**Root cause**: confirmed in the emitted IR. A by-value `for` over an array disowns the array at
the loop head (`mark_moved_for_drop` clears the array's flag and each `string` position's flag)
and registers the loop binding with `register_owned_binding`, which registers nothing for a bare
`string`: whether a `string` owns its buffer is a per-position runtime flag, and the binding is
given none. No iteration releases its element, and the array no longer does.

**Workaround**: iterate by index (`for i in 0..2` reading `parts[i]`) or by reference
(`for part in &parts`); both leave the array to release its elements.

**Fix sketch**: before clearing the array's position flags, copy them into a runtime-indexable
slot, and give the loop binding an owned flag armed from the slot at the current index, released
at the iteration's end and cleared if the body moves the binding. Elements that hold `string`
fields (a struct, a tuple) need the same per position. Regression tests: the repro under a leak
check, a literal element (must not be freed), and a body that moves the element into a `Vec`.

## BUG-098: `@derive(Clone)` refuses a struct with a field that owns a buffer

- **Status**: open, confirmed. Narrowed: the derived `.clone()` used to copy such a field's
  pointer, so the copy and the original shared one buffer, freed twice for a tensor, `Vec` or
  `StringBuilder` field and read after release for a `string` one. That is now a compile error
- **Area**: `semantic-analysis` (`is_derived_cloneable`) and `llvm-backend` (`StructClone` in
  `codegen/expressions/methods.rs`)
- **Severity**: minor. The compiler rejects, it does not miscompile

**Minimal repro**

```neuro
@derive(Clone)
struct Named { name: string, id: i32 }

func main() -> i32 {
    val a = Named { name: "a" + "b", id: 1 }
    val b = a.clone()
    b.name.len() as i32
}
```

Expected: exit 2. The language reference says `@derive` writes the impl a programmer would have
written, field by field, so `name` is copied with its own `.clone()`. Observed:

```text
struct 'Named' cannot derive `Clone`: field 'name' has type string, which owns a buffer that a
derived `.clone()` cannot copy yet; write `impl Clone` by hand
```

A tensor, `Vec`, `StringBuilder`, an array of `string`, and a generic field instantiated with any
of them are refused the same way.

**Root cause**: confirmed in the code. The backend's derived clone loads the struct's bytes. That
is a faithful copy only for fields that are `Copy` or are themselves structs cloned that way.

**Workaround**: write `impl Clone` by hand, calling `.clone()` on each owning field. The hand
impl meets a `T: Clone` bound exactly as the derive would.

**Fix sketch**: generate the derived clone field by field: a `string` or tensor field through its
own clone, a nested struct recursively, an array or tuple per element. A field type with no
`.clone()` of its own (`Vec`, `StringBuilder`) stays refused. The copy's `string` positions must
be armed, or the new buffers leak (see BUG-092). Regression tests: each field kind under a leak
check, the copy outliving the original, and a generic struct instantiated with `string`.

## BUG-085: a struct passed by value never releases the `string` buffers it holds

- **Status**: open, confirmed
- **Area**: `llvm-backend`; parameter registration in `codegen/functions.rs` and
  `plan_held_drops` in `codegen/drops/mod.rs`
- **Severity**: major. An unbounded leak, one buffer per call, for an ordinary by-value
  argument or a consuming `self` receiver

**Minimal repro**

```neuro
struct C { label: string, n: i32 }

impl C {
    func into_n(self) -> i32 { self.n }
}

func take(c: C) -> i32 { c.n }

func main() -> i32 {
    mut t = 0
    for i in 0..10 {
        val a = C { label: "a" + "bc", n: 1 }
        t = t + a.into_n()
        val b = C { label: "a" + "bcd", n: 1 }
        t = t + take(b)
    }
    t
}
```

Expected: exit 20, with every `label` buffer released once, by the callee that took ownership of
the struct. Observed: exit 20, and AddressSanitizer's leak check reports every buffer as leaked.
`examples/showcase/owned_catalog.nr` shows it: its `catalog.into_report()` is commented as
taking the field's buffer with it, and that buffer leaks. The one leak left in `examples/showcase/log_builder.nr`
is this shape too: a consuming `self` method moving a `string` field out.

**Root cause**: confirmed in the code. At the call the caller clears the drop flags of the
struct it moves, including the flags of its `string` fields. The callee registers the
parameter for destruction, but `plan_held_drops` starts every `string` position disarmed,
because a type cannot prove a `string` owns its buffer. Nothing re-arms the position, so the
callee skips the release, and the caller already gave it up.

**Workaround**: pass the struct by `&` and let the caller keep ownership. Moving the field
into a binding in the callee (`val s = c.label`) does not help: that binding owns nothing
either.

**Fix sketch**: the flags need to cross the call. Either pass each `string` position's flag as a
hidden argument, or have the caller keep ownership of the positions when the callee's summary
proves it only reads them (the way a read-only `string` parameter already works) and release
them after the call. Regression tests: the repro in a loop under a leak check, a literal field
that must not be freed, and a field the callee moves out and returns.

## BUG-082: an enum that owns a `string` never releases it

- **Status**: open, confirmed. Narrowed: an enum holding an owner now moves rather than
  copies, so the double free this entry was filed for is a compile error
- **Area**: `llvm-backend` (`enum_holds_owner` in `codegen/drops/mod.rs`)
- **Severity**: major. An unbounded leak, one buffer per evaluation

**Minimal repro**

```neuro
enum Msg {
    Text(string),
    Num(i32)
}

func make() -> u64 {
    val m = Msg::Text("ab" + "cd")
    7
}

func main() -> i32 {
    mut t: u64 = 0
    for i in 0..5 {
        t = t + make()
    }
    if t != 35 { return 1 }
    0
}
```

Expected: each buffer `"ab" + "cd"` built is released when `m` leaves scope. Observed: the
arithmetic is right, and AddressSanitizer's leak check reports the 4-byte buffers as leaked.

The prelude's enums hit it too. A `Result<string, E>` holding a computed string leaks through `?`,
`??`, `match` and a plain drop, and an enum payload that is a struct whose only owner is a `string`
leaks the same way, because `holds_owner` answers `false` for that struct.

**Root cause**: confirmed in the code. `enum_holds_owner` asks `holds_owner` of each payload
type, which answers `false` for `string` by design: a `string` position owns its buffer only
when the store that filled it allocated one, which is a runtime fact. A struct field carries
that fact in a drop flag per position; an enum payload slot has none, because which payload
is live depends on the tag.

**Workaround**: hold the text in a struct field, or build it where it is read.

**Fix sketch**: give an enum's `string` payload positions a flag of their own, armed at the
construction when the payload expression allocates, cleared when a `match` binds the payload
by value, and consulted by the enum's drop. A payload read through a borrow must leave it set.
Regression tests: the repro, a payload built from a literal (must not be freed), and a payload
moved out by a `match` (freed once, by the binding).

## BUG-077: a store through a borrow leaks the `string` or tensor it displaces

- **Status**: open, confirmed. Narrowed: a displaced value whose type proves what it owns (a
  user `Drop` type, a `Vec`, a map, or a holder of one) is now destroyed at the store
- **Area**: `llvm-backend`; `drop_displaced_through_borrow` in `codegen/drops/mod.rs`
- **Severity**: major. An unbounded leak, one buffer per store, through a `&mut` parameter, a
  `&mut self` receiver or a `*r` store

**Minimal repro**

```neuro
struct Named { name: string }

func rename(n: &mut Named, i: i32) { n.name = "item {i}" }

func main() -> i32 {
    mut n = Named { name: "a" + "b" }
    mut i = 0
    while i < 1000 {
        rename(&mut n, i)
        i += 1
    }
    n.name.len() as i32
}
```

Expected: each `rename` releases the buffer it displaces, and the last one is released at `n`'s
scope exit. Observed: exit 8, and AddressSanitizer's leak check reports every displaced buffer,
the first `"a" + "b"` included. A tensor field replaced through a borrow leaks the same way.

The stored value can leak too. A store through a borrow copies any `string` it cannot prove
owned to the heap (`own_strings_stored_through_borrow`), so the owner's release never frees a
`.rodata` literal, but the owner's flag stays clear, so nobody releases the copy:
`func set(t: &mut (string, i32), v: string) { t.0 = v }` called as `set(&mut b, "abc")` in a
loop leaks one copy per call, for a plain function and a generic instance alike.

**Root cause**: confirmed in the code. Whether a `string` position owns its buffer is a runtime
flag of the binding that owns the place, and a store through a borrow runs in a frame that
cannot see it, so it cannot tell a heap buffer from a `.rodata` literal and leaves the displaced
value alone. A tensor is left alone for a different reason: the value behind the borrow may have
come from a `pool` arena, and a callee cannot see whether its caller is inside one.

**Workaround**: store through the owning binding (`n.name = ...` in the scope that owns `n`).

**Fix sketch**: carry the flags with the reference, as a hidden argument per `string` position
of a `&mut` parameter, or have the callee report which positions it wrote so the caller
releases what they held before the call. Regression tests: the repro in a loop under a leak
check, a literal in the place (must not be freed), and a tensor field inside and outside a
`pool`.

## BUG-027: a const generic parameter cannot be passed to another generic call

- **Status**: open, confirmed
- **Area**: `semantic-analysis` (generic inference, `unify_array_len` / `seed_turbofish`)
- **Severity**: major. A generic function cannot delegate to another over its own const
  parameter, by inference or explicitly; the compiler rejects, it does not miscompile

A generic function that takes a const parameter (including a tensor shape parameter, which
the language reference makes sugar for one) cannot pass that parameter to another generic
function. Type parameters forward correctly; only const parameters fail.

**Minimal repro**

```neuro
func sum_n<N>(t: &Tensor<i32, [N]>) -> i32 {
    mut total = 0
    for i in 0..(N as i32) { total = total + t[i] }
    return total
}

func delegate<N>(t: &Tensor<i32, [N]>) -> i32 {
    return sum_n(t)
}

func main() -> i32 {
    val v: Tensor<i32, [3]> = [1, 2, 3]
    return delegate(&v)
}
```

Expected: compiles and returns 6. The callee's `N` is named by the argument's own type, so
it is inferable. Observed:

```text
generic parameter 'N' cannot be inferred from the call arguments;
supply it explicitly with a turbofish, e.g. `f::<...>(...)`
```

The turbofish the message recommends does not work either. `sum_n::<N>(t)` reports

```text
turbofish argument for parameter 'N' has the wrong kind: a const argument was expected
```

so the parameter can be supplied neither way. Calling the same function from a *concrete*
caller works, and the identical program over a type parameter (`func outer<T>(x: T) -> T {
inner(x) }`) works, which is what isolates this to const parameters. Arrays hit it too: a
`func delegate<const N: u32>(a: &[i32; N])` calling a `func sum_arr<const N: u32>` fails the
same way.

A const parameter named as a generic *type's* argument fails the same way. Inside
`impl<T, const CAP: u32> Buffer<T, CAP>`, `Self` and `Buffer<T, CAP>` both report
`unknown type name 'CAP'`, because a generic argument that is a bare name resolves as a type.
Type parameters work in that position, so `Self` in `impl<T> Cell<T>` is fine.

**Root cause**: two halves, both confirmed in the code.

Inference: `unify_array_len` (`type_checkers/declarations/mod.rs`) matches a symbolic callee
extent against a symbolic argument extent with `(ArrayLen::Param(a), ArrayLen::Param(b)) =>
a == b`. It reports agreement but inserts nothing into the substitution, so the later
"every parameter must be bound" loop in `calls.rs` finds no binding and reports the
parameter as uninferable. When the two spell the parameter differently the same arm returns
`false` and an extent mismatch is reported on top.

Turbofish: `seed_turbofish` accepts a const argument only as `GenericArg::Const`, which is
what the parser produces for a literal. An identifier naming an in-scope const parameter
parses as `GenericArg::Type`, so it lands in the kind-mismatch arm.

**Workaround**: none within a generic function. Inline the callee's body, or make the caller
concrete.

**Fix sketch**: this is bigger than the two arms it appears to be, which is why it is filed
rather than patched. A substitution has to be able to hold a *symbolic* const ("the
caller's parameter `N`") not only a `Type::ConstValue`; `unify_array_len` then binds the
callee's parameter to it, `substitute_array_len` maps it back to an `ArrayLen::Param` in the
caller's frame, and the Copy check in `calls.rs` needs a carve-out for it the way
`ConstValue` already has one. `seed_turbofish` separately has to resolve an identifier
argument against the enclosing function's const parameters before deciding its kind. The
part to settle first is monomorphization: an instantiation of the outer function has to
resolve the inner call's symbolic binding to the concrete extent it was instantiated at, and
whether the existing instantiation walk carries enough context to do that is the question
that decides the shape of the rest. Regression tests want both spellings of the parameter
name, the turbofish form, the array form, and two distinct instantiations of the outer
function so a wrong extent could not pass unnoticed.

## Taking one of these on

An entry here is a self-contained task: repro, root cause where known, and a fix
sketch. If you want to work on one:

1. Open (or claim) an issue naming the `BUG-NNN` id so work isn't duplicated.
2. Follow the normal [contribution workflow](../CONTRIBUTING.md), branch, tests,
   quality gates, DCO sign-off.
3. A bug fix **must** ship with a regression test that fails without the fix. Name it
   after the defect so its purpose is obvious.
4. If an entry turns out to be a language-design decision rather than a patch, say so in
   the issue, some of these need a maintainer ruling before code changes.
