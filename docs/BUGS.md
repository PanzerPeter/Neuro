# Known Bugs

Open defects only, newest first. Every confirmed bug that is not yet fixed has a numbered
`BUG-NNN` entry here; when a bug is fixed its entry is **deleted**, the fix lives in
`CHANGELOG.md`, in the affected slice's `CONTEXT.md`, and in its regression test. IDs are
never reused, so numbering stays stable as entries are removed.

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

## BUG-110: an `impl` bounded by `Copy` or `Clone` emits nothing for a primitive argument

- **Status**: open, confirmed
- **Area**: `hir-lowering`; `instance_meets_impl_bounds` in `items/mono.rs`
- **Severity**: major. A program the checker accepts fails to build with an internal codegen
  error

**Minimal repro**

```neuro
struct Box2<T: Copy> { v: T }
impl<T: Copy> Box2<T> {
    func make(v: T, w: T) -> Box2<T> { Box2 { v: w } }
}
func main() -> i32 {
    val b = Box2::make(2, 3)
    b.v
}
```

Expected: the program builds and exits with 3. Observed: `Code generation error: undefined
function: Box2$g_i32__make`. The same program without the two `: Copy` bounds builds.

**Root cause**: confirmed in the code. Lowering emits an instance's `impl` only when every bound
holds, and it decides that from `trait_impls` alone: a trait the program (here, the prelude)
declares is met only by a struct, enum or newtype with a matching `impl`. A primitive, tuple or
array argument therefore fails `T: Copy`, `T: Clone`, `T: PartialEq` or `T: Hashable`, which the
checker's `satisfies_bound` grants it, and the impl's functions are never emitted.

**Workaround**: drop the bound from the type and its `impl` when the body does not need it.

**Fix sketch**: give lowering the checker's answer for the derivable traits on non-nominal
types (scalars, `char`, `bool`, and tuples and arrays of such), or record which bounds the
checker found satisfied per instance. Regression test: the repro above, plus the same with a
`(i32, i32)` argument.

## BUG-109: a reference assigned inside a block may outlive what it borrows

- **Status**: open, confirmed
- **Area**: `semantic-analysis`; the assignment path that makes a `mut` reference binding hold
  its new borrow (`attach_borrow` in `type_checkers/statements/places.rs`)
- **Severity**: major. A silent use after free: the program compiles and reads released memory

**Minimal repro**

```neuro
func main() -> i32 {
    val t: Vec<i32> = Vec::new()
    mut b: &Vec<i32> = &t
    {
        mut v: Vec<i32> = Vec::new()
        v.push(41)
        b = &v
    }
    return b[0]
}
```

Expected: a compile error, since `v` dies at the end of the block while `b` still borrows it.
Observed: it compiles, `v`'s buffer is released at the brace, and `b[0]` reads it (the run exits
with a garbage value, not 41).

**Root cause**: confirmed in the code. The assignment makes `b` hold a borrow of `v`, but nothing
compares the two bindings' scopes: a borrow is checked for conflicts while both live, never for a
holder declared outside the borrowee's scope.

**Workaround**: declare the reference inside the block that owns what it borrows.

**Fix sketch**: refuse an assignment that makes a binding hold a borrow of a place declared in a
deeper scope than the binding itself (the `BorrowProvenance::scope` the symbol table already
records gives both depths). Regression test: the repro above, and the same assignment where `v`
is declared beside `b`, which must keep compiling.

## BUG-099: a struct returned through a binding may hold a borrow of its locals

- **Status**: open, confirmed. Narrowed: a struct, enum or tuple literal, an enum variant
  constructor or an array returned directly is now checked, field by field, and so is each arm of
  a tail `if`; borrowing a local that way is the "cannot return a reference" error
- **Area**: `semantic-analysis`; `check_returned_reference` in `type_checkers/statements/returns.rs`
- **Severity**: major. A silent use after free: the program compiles and reads released memory

**Minimal repro**

```neuro
struct H { r: &string }

func make() -> H {
    val s = "he" + "llo"
    val h = H { r: &s }
    h
}

func main() -> i32 {
    val h = make()
    h.r.len() as i32
}
```

Expected: a compile error, as for `H { r: &s }` written as the tail. Observed: it compiles, `s` is
released when `make` returns, and `h.r` reads the freed buffer.

**Root cause**: confirmed in the code. The check walks the returned expression down to each
`&place`. A binding is a name at that point, and the borrow model records no borrow held inside a
struct binding, so there is nothing to follow.

**Workaround**: return the literal itself, or borrow only from parameters.

**Fix sketch**: track the borrows a struct or enum binding holds, which is the work of the
"References in struct fields and enum payloads" roadmap item, and close this with it. Regression
test: the repro above, its enum form, and a binding that borrows a parameter, which must keep
compiling.

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

## BUG-033: `&` does not accept a field or an element, only a bare variable

- **Status**: open, confirmed
- **Area**: `semantic-analysis` (borrow checking); the rvalue side of the place-expression
  machinery the assignment target already resolves
- **Severity**: major. It is what a layer type is written with, and the only escape
  copies the buffer that move-by-default exists to avoid copying.

`&x` type-checks only when `x` is a bare identifier. `&p.field` and `&arr[i]` are rejected
as "not a place", for every element type, so a value held in a struct field cannot be
borrowed at all. For a non-`Copy` field the two available spellings close on each other:
the borrow is refused as not a place, and the bare field is refused as a move out of a
`&self`.

**Minimal repro**

```neuro
struct Layer { w: Tensor<i32, [2, 2]> }

impl Layer {
    func forward(&self, x: &Tensor<i32, [2, 2]>) -> Tensor<i32, [2, 2]> {
        &self.w @ x
    }
}

func main() -> i32 {
    val l = Layer { w: [[1, 2], [3, 4]] }
    val x: Tensor<i32, [2, 2]> = [[5, 6], [7, 8]]
    return l.forward(&x)[0, 0]
}
```

Expected: compiles and returns 19. Observed:

```text
cannot borrow this expression: `&` requires a place (a variable); bind it to a `val` first
```

Dropping the `&` swaps one error for the other:

```text
cannot move out of 'self': it is reached through a `&` borrow, which owns nothing to give
away; bind a `.clone()` instead, or take the value by a binding that owns it
```

A scalar field shows the same restriction without the second half: `take(&p.a)` on a
`struct Pair { a: i32 }` is rejected, and so is `take(&arr[0])`.

**Root cause**: not yet confirmed in the code. The borrow check recognises exactly one
place form, a name in the symbol table, so a field access or an index expression never
reaches it.

**Workaround**: `self.w.clone()`, which the move diagnostic names and which does compile
and produce the right answer. It allocates a second buffer of the same shape on every
call, so it is a workaround for correctness and not for a training loop.

**Why this is still open**: the value-model sub-phase's "place expressions" item shipped
the ASSIGNMENT half, so `t[i, j] = v` and `self.x += dx` now compile and `ast_types::Place`
names the storage they write to. The rvalue borrow was not part of that item and is the
half a layer type needs first.

**Fix sketch**: give the borrow check the same notion of a place the assignment target
already resolves (`TypeChecker::resolve_place`), and reach the address in codegen through
`CodegenContext::held_place_ptr`, which already resolves a binding, a field at any depth,
an array element and a tuple element to storage. Both halves of the machinery now exist;
what is missing is the borrow check accepting a sub-place as the operand of `&` and
`&mut`, and the exclusivity bookkeeping for a borrow of part of a binding rather than the
whole of it.

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
