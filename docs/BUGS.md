# Known Bugs

Open defects only, newest first. Every confirmed bug that is not yet fixed has a numbered
`BUG-NNN` entry here; when a bug is fixed its entry is **deleted**, the fix lives in
`CHANGELOG.md`, in the affected slice's `CONTEXT.md`, and in its regression test. IDs are
never reused, so numbering stays stable as entries are removed.

## BUG-155: a panic in a standard-library body or an imported module reports a position in the program's file

- **Status**: open, confirmed
- **Area**: `llvm-backend`, panic locations
- **Severity**: minor. The message is right and the program aborts as it should, but the location
  points at the wrong file

**Minimal repro**

```neuro
func main() -> i32 {
    val z = 0.0
    val o = OrderedF64::new(z / z)
    0
}
```

Expected: a location naming the `new` call, or the standard library's own source. Observed:
`panic: OrderedF64 cannot hold NaN at main.nr:6:1`, a line past the end of a five-line file. A
standard-library body's spans are offsets into its own embedded source, and the panic location
reads them against the program's file. A zero stride passed to a range value's `.step(n)` at run
time reports the same kind of position, and so does a `panic` in any imported module of a
multi-file program: a `Span` is a pair of byte offsets with no file, and the merged program has
no way to say which file one indexes.

**Workaround**: none. The message text still names the cause.

**Fix sketch**: give a span the module it belongs to, which the hierarchical module namespaces
on the roadmap need anyway, and resolve the panic location against that module's source. A
narrower stopgap reports a standard-library panic at the call site that entered it.

## BUG-152: a method returning its own type nested in itself does not compile

- **Status**: open, confirmed. Narrowed: the checker now stops the chain at 128 nested instances
  with a diagnostic, the depth lowering already enforced, instead of allocating until killed
- **Area**: `semantic-analysis` (`instantiate_impls_for`); `hir-lowering` (`register_instance_methods`)
- **Severity**: major. A well-typed program fails to compile

**Minimal repro**

```neuro
struct W<I> { inner: I }

impl<I> W<I> {
    func wrap(self) -> W<W<I>> { W { inner: self } }
}

func main() -> i32 {
    val w = W { inner: 3 }
    w.wrap().inner.inner
}
```

Expected: exit code 3. Observed: `instantiating 'W' nests generic instances more than 128 deep`.
Before the cap, `neurc check` allocated until the memory cap aborted it. Every instance
registers the signature of each method of its generic impls, and registering `wrap` for `W<i32>`
builds `W<W<i32>>`, whose own `wrap` builds `W<W<W<i32>>>`, with no end. A trait's default method
returning `Wrapper<Self>` does the same, which is why the prelude's iterator adapters are not default
methods on `Iterator`.

**Workaround**: none that keeps the method; a free function `func wrap<I>(w: W<I>) -> W<W<I>>`
instantiates per call and terminates.

**Fix sketch**: register an instance's method signatures when a call first looks the method up, not
when the instance is built, in the checker and in lowering's `register_instance_methods` alike.

## BUG-151: a `break` value loses the borrow it carries

- **Status**: open, confirmed
- **Area**: `semantic-analysis`, held borrows
- **Severity**: major. The borrowee can be moved or freed while the loop's value still reads it

**Minimal repro**

```neuro
func main() -> i32 {
    val s = "ab" + "c"
    val r = loop {
        break &s
    }
    val t = s
    println(r)
    0
}
```

Expected: moving `s` is refused while `r` holds it. Observed: the program compiles, because the walk
that finds what a value holds stops at a `loop`, so `r` holds nothing.

**Workaround**: assign to a binding declared before the loop and `break` without a value.

**Fix sketch**: collect each `break` value's borrows while the loop is checked, keyed by the loop's
span, and let `hold_carried_borrows` read them. `refuse_borrowing_break` can then go, which refuses
the same shape for function values.

## BUG-150: a field read off a temporary struct leaks the struct's `string`

- **Status**: open, confirmed
- **Area**: `llvm-backend`, statement temporaries
- **Severity**: minor. One buffer per evaluation of the expression; the program still runs

**Minimal repro**

```neuro
struct Run { name: string }

func mk(s: &Run) -> Run {
    Run { name: "{s.name}'" }
}

func main() -> i32 {
    val a = Run { name: "ab" }
    println("{mk(&a).name}")
    0
}
```

Expected: the temporary `Run` releases `name` when the statement ends. Observed: LeakSanitizer
reports the 3 bytes, allocated in `mk`.

**Workaround**: bind the value first, `val r = mk(&a)`, and read `r.name`.

**Fix sketch**: hold the receiver of a field read that is not a place to the statement's end, as a
temporary `&self` receiver already is.

## BUG-149: a plain binding copies a `&mut`, so two exclusive borrows of one value live at once

- **Status**: open, confirmed
- **Area**: `semantic-analysis` (`copy_within` in `type_checkers/mod.rs`)
- **Severity**: major. Two live `&mut` borrows alias one value, which the borrow checker must refuse

**Minimal repro**

```neuro
func main() -> i32 {
    mut n = 3
    val a = &mut n
    val b = a
    val c = a
    *b = 1
    *c = 2
    0
}
```

Expected: `a` moves into `b`, and `val c = a` is a use after move. Observed: it compiles and both
writes run. A `@derive(Copy)` generic struct holding a `&mut` copies the same way. A `T: Copy` bound
does refuse a `&mut`.

**Workaround**: none in the compiler. Do not bind a `&mut` twice.

**Fix sketch**: `&mut` moves, with an implicit reborrow where a `&mut` parameter takes one, so that
`copy_within` can answer `false` for it without breaking every call that passes a `&mut` on.

## BUG-146: a loop inside a `pool` releases only its last `PoolAware` value

- **Status**: open, confirmed
- **Area**: `llvm-backend` (`codegen/arena.rs`, `codegen_var_decl`)
- **Severity**: major. External resources leak silently; the program still runs

**Minimal repro**

```neuro
struct Handle { id: i32 }
impl PoolAware for Handle {
    func register_with_pool(&self, arena: &PoolHandle) { println("register {self.id}") }
    func bulk_release(&mut self) { println("release {self.id}") }
}
func main() -> i32 {
    pool {
        mut i = 0
        while i < 3 {
            val h = Handle { id: i }
            i = i + 1
        }
    }
    0
}
```

Expected: `release 2`, `release 1`, `release 0`. Observed: three registrations and only
`release 2`.

**Root cause**: confirmed in the code. A registration cell points at the binding's stack slot
and its drop flag, and a loop reuses both on every iteration. The sweep sees the last value
through every cell, releases it once, clears the shared flag and skips the rest.

**Workaround**: open the `pool` inside the loop, so each iteration sweeps its own value.

**Fix sketch**: give each registration its own storage and flag, by placing a registered
binding in the arena or by copying the value there when its scope ends. Either must keep a
mutation made after registration visible to `bulk_release`. Regression test: the repro, plus
a `break` and a moved value.

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

A `string` binding moved into a payload (`Option::Some(w)`) belongs to the enum from then on, so it
leaks with the enum, even when a `match` in the same function takes it out again: an arm's `string`
binding owns nothing. Before, the scope that declared `w` freed it. That was correct only when the
enum died in the same scope; any enum that left it read a freed buffer.

The prelude's enums hit it too. A `Result<string, E>` holding a computed string leaks through `?`,
`??`, `match` and a plain drop, and an enum payload that is a struct whose only owner is a `string`
leaks the same way, because `holds_owner` answers `false` for that struct.

**Root cause**: confirmed in the code. `enum_holds_owner` asks `holds_owner` of each payload
type, which answers `false` for `string` by design: a `string` position owns its buffer only
when the store that filled it allocated one, which is a runtime fact. A struct field carries
that fact in a drop flag per position; an enum payload slot has none, because which payload
is live depends on the tag.

**Workaround**: hold the text in a struct field, or build it where it is read.

**Fix sketch**: a flag per payload position cannot cross a call, so it would not reach
`Result<string, E>`. Keeping the owner bit in the `string` value itself would, along with BUG-085
and BUG-077; that is an open decision about the string representation. Regression tests: the repro,
a payload built from a literal (must not be freed), and a payload moved out by a `match` (freed
once, by the binding).

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
