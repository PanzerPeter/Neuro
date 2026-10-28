# Known Bugs

Open defects only, newest first. Every confirmed bug that is not yet fixed has a numbered
`BUG-NNN` entry here; when a bug is fixed its entry is **deleted**, the fix lives in
`CHANGELOG.md`, in the affected slice's `CONTEXT.md`, and in its regression test. IDs are
never reused, so numbering stays stable as entries are removed.

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

## BUG-129: a generic function taking `&[T]` accepts no argument

- **Status**: open, confirmed
- **Area**: `semantic-analysis`; generic call checking, `unsizes_to_slice` in `type_checkers/resolution.rs`
- **Severity**: major. A documented signature, `func sort<T: Comparable>(xs: &mut [T])`, cannot be called

**Minimal repro**

```neuro
func first<T: Copy>(xs: &[T]) -> T { xs[0] }

func main() -> i32 {
    val a = [4, 5]
    first(&a)
}
```

Expected: exit 4. Observed: `type mismatch: expected &[T], found &[i32; 2]`. The same error
follows for `first::<i32>(&a)` and for an argument that is already a slice (`a.slice(0..2)`,
`found &[i32]`), so neither inference nor a turbofish reaches the parameter.

**Root cause**: suspected, not traced end to end. The argument is checked against the
declared `&[T]` with `T` still a generic, and the unsizing rule compares element types with
`is_compatible_with`, which does not bind or substitute a type parameter. A non-generic
`func f(xs: &[i32])` accepts `&a`.

**Workaround**: write the function for the concrete element type, or take `&[T; N]` with a
const parameter.

**Fix sketch**: unify the slice element with the argument's element (array, `Vec` or slice)
when inferring the call's type arguments, and check the argument against the substituted
parameter type. Regression tests: the repro, the turbofish form, a `Vec` argument and a
`&mut [T]` parameter written through.

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

## BUG-127: a borrow passed to an inner call blocks a move in the enclosing call

- **Status**: open, confirmed
- **Area**: `semantic-analysis` (`reject_move_of_borrowee` in `type_checkers/moves.rs`)
- **Severity**: minor. A valid program is rejected; binding the inner result first compiles

**Minimal repro**

```neuro
func len_of(s: &string) -> u64 { s.len() }
func take(n: u64, s: string) -> u64 { n + s.len() }

func main() -> i32 {
    val key = "abc"
    val n = take(len_of(&key), key)
    n as i32
}
```

Expected: exit 6. `len_of` returns a `u64`, so the borrow of `key` ends when it returns, before
`key` is moved as the second argument. Observed: "cannot move out of 'key' while it is
borrowed". A method chain hits the same check: `lookup(&key).ok_or(Missing(key))`.

**Root cause**: confirmed in the code. A call's argument borrows are transient, counted while
the call's operands are checked, and `reject_move_of_borrowee` counts every transient borrow on
purpose (a `&mut` in flight beside a move of the same name is a real dangling borrow). The
inner call's borrows are still counted when the enclosing call's next operand is checked,
although the inner call has returned and its result holds no reference.

**Workaround**: bind the inner call's result with `val` first.

**Fix sketch**: release a call's transient argument borrows once the call is checked, unless
its result type can hold a reference, in which case they pass to the result as they do today.
Regression tests: the repro, the method-chain form, and a `&mut key` passed beside a move of
`key` in the same call (must still be rejected).

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

## BUG-124: a `for` over an array leaks its last element when it was not built in place

- **Status**: open, confirmed
- **Area**: `llvm-backend`, ownership of `string` positions in an array iterated by `for`
- **Severity**: minor. One buffer leaks per evaluation of the literal; no value is wrong

**Minimal repro**

```neuro
func q() -> string { "a" + "b" }
func main() -> i32 {
    val parts = ["x" + "y", q()]
    mut total = 0
    for part in parts {
        total += part.len() as i32
    }
    total
}
```

Expected: exit 4 and both buffers released, as `parts[0].len() + parts[1].len()` does with the
same literal. Observed: exit 4, and LeakSanitizer (`LD_PRELOAD` of `libasan.so`) reports the
buffer `q()` returned. A moved binding in the last position (`["x" + "y", b]` with
`val b = q()`) leaks the same way, and so does `[q(), q()]` (one buffer). `[q(), "x" + "y"]`
and `[q()]` leak nothing.

**Root cause**: not yet confirmed in the code. The element's position is armed when the
literal is bound (indexing the same binding releases both), so the leak is on the path that
moves elements out of the array into the loop binding.

**Workaround**: none needed for correctness. Iterating by index (`for i in 0..2` reading
`parts[i]`) does not leak.

**Fix sketch**: compare the per-element flags the `for` head hands to each iteration's
binding against the flags the declaration armed, for a last element that was not allocated
in place.

## BUG-118: calling a function-valued field in parentheses fails to build

- **Status**: open, confirmed
- **Area**: `llvm-backend`, call lowering of a parenthesized callee
- **Severity**: major. A program the checker accepts fails to build with an internal codegen
  error

**Minimal repro**

```neuro
struct K { f: (i32) -> i32 }
func double(x: i32) -> i32 { x * 2 }
func main() -> i32 {
    val k = K { f: double }
    (k.f)(21)
}
```

Expected: the program builds and exits with 42, as `val g = k.f` then `g(21)` does. Observed:
`Code generation error: undefined function: K__f`. The indexed form `(ks[0].f)(21)` fails the
same way.

**Root cause**: not yet confirmed in the code. The call is lowered as a method call `K__f` on the
receiver rather than as an indirect call through the field's value.

**Workaround**: bind the field first: `val g = k.f` then `g(21)`.

**Fix sketch**: when a call's callee is a parenthesized field access whose field is
function-typed, read the field and call through the value, as a call through a local does.

## BUG-117: a closure written before a `pool` and called inside it can keep the pool's memory

- **Status**: open, confirmed
- **Area**: `semantic-analysis`, the `pool` retention rule for a call through a closure value
- **Severity**: critical. Silent wrong answer: the outer value reads memory the arena has
  already handed to a later pool

**Minimal repro**

```neuro
struct Holder { t: Tensor<f32, [1024]> }
func main() -> i32 {
    mut out = 0.0f32
    mut h = Holder { t: Tensor::<f32, [1024]>::zeros() }
    {
        val f = |n: Tensor<f32, [1024]>| { h.t = n }
        pool { f(Tensor::<f32, [1024]>::ones()) }
    }
    pool { val z = Tensor::<f32, [1024]>::zeros()
           out = z.sum() }
    out = out + h.t.sum()
    (out / 8.0f32) as i32
}
```

Expected: 128 (1024 ones summed, divided by 8), or a refusal of the call, as `h.t = ...` written
directly in the pool is routed off the arena. Observed: 0. The tensor built in the first pool's
arena is stored into `h` through the closure's mutable capture, the arena is released at the
pool's exit, and the second pool reuses that memory.

**Root cause**: not yet confirmed in the code. The retention rule reads what a call can store
from the callee's parameters; a closure's captures are a second channel it does not see.

**Workaround**: assign the field directly inside the pool, or build the value outside it.

**Fix sketch**: treat a call through a closure that captures a binding declared before the pool
by mutable borrow as able to store each argument into that binding, and judge it like a `&mut`
argument.

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

## BUG-092: a struct returned from a function never releases the `string` buffers it holds

- **Status**: open, confirmed
- **Area**: `llvm-backend`; `plan_held_drops` in `codegen/drops/mod.rs`, the same mechanism as
  BUG-085 in the return direction
- **Severity**: major. An unbounded leak, one buffer per call, for a struct built by a helper

**Minimal repro**

```neuro
struct T { text: string, n: i32 }

func mk(i: i32) -> T { T { text: "w {i}", n: i } }

func main() -> i32 {
    mut c: u64 = 0
    for i in 0..4 {
        val t = mk(i)
        c = c + t.text.len()
    }
    c as i32
}
```

Expected: exit 12, with each `text` buffer released when `t` leaves scope. Observed: exit 12,
and AddressSanitizer's leak check reports all four buffers. Building the struct in place
(`val t = T { text: "w {i}", n: i }`) releases every buffer, so the two spellings disagree.
Moving the field on (`v.push(t.text)`, `val s = t.text`) leaks the same way, which is what
`examples/showcase/word_scanner.nr` does with `Token::of`.

**Root cause**: confirmed in the code. The callee clears its own drop flags when the struct
leaves as its return value. The caller's binding gets its positions from `plan_held_drops`,
which starts every `string` position disarmed, because a type cannot prove a `string` owns its
buffer, and only a store in the caller's frame arms one. A binding initialized from a call
result has no such store, so nothing releases the field.

**Workaround**: build the struct where it is used, or return the `string` itself and build the
struct in the caller.

**Fix sketch**: the flags have to cross the return, as BUG-085 needs them to cross the call:
return the `string` positions' flags beside the struct, or have the callee's summary prove that
every return path fills a position with a fresh buffer and arm it in the caller. Regression
tests: the repro in a loop under a leak check, a field filled from a literal (must not be
freed), and a field moved out of the returned struct.

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
