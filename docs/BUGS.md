# Known Bugs

Open defects only, newest first. Every confirmed bug that is not yet fixed has a numbered
`BUG-NNN` entry here; when a bug is fixed its entry is **deleted**, the fix lives in
`CHANGELOG.md`, in the affected slice's `CONTEXT.md`, and in its regression test. IDs are
never reused, so numbering stays stable as entries are removed.

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

## BUG-088: an attribute the compiler does not know is accepted and ignored

- **Status**: open, specification gap
- **Area**: `semantic-analysis`; attributes are read by name where each one matters
  (`grad`, `no_grad`, `gpu`, `kernel`, `derive`, `allow`) and never checked as a set
- **Severity**: minor. Nothing miscompiles, but a misspelled attribute silently does nothing

**Minimal repro**

```neuro
@no_grda
func scale() -> f32 { 2.0f32 }

func main() -> i32 {
    scale() as i32
}
```

Observed: compiles and exits 2. The misspelled `@no_grda` is dropped, so inside a `@grad` body
the call would be differentiated rather than held constant. `@gpu` and `@kernel` are no longer
part of this: each one's form is checked, and a body that cannot run on a GPU is a compile
error.

**Open question for the specification**: the custom attributes section says the `@name(args)`
syntax is extensible, and says nothing about a name no one defined. Either an unknown attribute
is an error (the usual choice, and the one that keeps a typo from changing a program's meaning),
or it is ignored.

**Root cause**: confirmed in the code. Each consumer looks for its own attribute name on the
item and skips everything else; no pass checks an item's attributes against the known set.

**Workaround**: none needed for correct spellings. Check attribute names by hand.

**Fix sketch**: once the rule is settled, one pass over every item's attributes against the
recognized names, reporting the unknown one at its span. Regression tests: a misspelled
`@no_grad`, and every recognized attribute still accepted.

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
taking the field's buffer with it, and that buffer leaks.

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

## BUG-050: calling a closure literal in place reports a function type as "non-function"

- **Status**: open, specification gap plus a wrong diagnostic
- **Area**: `semantic-analysis`; call checking in `type_checkers/expressions/calls.rs`
- **Severity**: minor. Nothing miscompiles; the program is refused with a message that
  contradicts itself

**Minimal repro**

```neuro
func main() -> i32 {
    val e = (|x: i32| -> i32 { x + 1 })(3)
    e
}
```

Observed: `error: cannot call non-function type fn(i32) -> i32`, followed by a cascade error
on every later use of `e`. The type the message prints IS a function type. Binding the closure
first (`val f = |x: i32| -> i32 { x + 1 }` then `f(3)`) compiles and returns 4.

**Open question for the specification**: the closures section says nothing about calling a
closure expression directly. Either it is legal, in which case this is a missing call path, or
it is not, in which case the diagnostic should say that a closure has to be bound before it is
called. Whichever is chosen, "non-function type" is wrong for a function type.

**Root cause**: not yet confirmed in the code. The call checker appears to accept a callee
that is a name or a path and to fall through to the non-function error for any other callee
expression, whatever its type.

**Workaround**: bind the closure to a `val` and call the binding.

## BUG-049: a `pool` refuses to store some values it could prove are heap memory

- **Status**: open, undecided (reproduces; whether the refusal is a defect or an accepted
  limit of the provenance walk is not settled)
- **Area**: `semantic-analysis`; `carries_no_arena` in `type_checkers/pools.rs`
- **Severity**: minor. Sound (the refusal never lets arena memory escape) but it rejects
  programs whose values never touch the arena

**Minimal repro**

```neuro
func main() -> i32 {
    val a: Tensor<i32, [2, 2]> = [[1, 2], [3, 4]]
    mut out: Tensor<i32, [2, 2]> = [[0, 0], [0, 0]]
    mut s: string = "none"
    val c = true
    pool scratch {
        out = a.map(|x: i32| -> i32 { x * 10 })   // refused
    }
    0
}
```

The store above is refused with "... outlives the pool". Written another way, the same
value is accepted: `out = &a + &a` and `out = einsum("ij->ij", a)` compile. A store into a
binding that outlives the block is emitted with the arena switched off, so none of these
values can hold arena memory unless an operand already did.

**Root cause**: confirmed in the code. `carries_no_arena` enumerates the expression shapes it
can prove, and falls back to "may carry arena memory" for everything else. A closure literal
argument, a method call such as `local.clone()` on a block-local receiver, and an `if` /
`match` arm or a block that declares a binding of its own are not enumerated, so each is
refused. Struct, tuple and array literals, and `if` / `match` whose arms are single
expressions, are proven.

**Workaround**: bind the value inside the block and copy out a scalar, or build it before the
block.

**Fix sketch**: admit a closure literal argument whose captures are all admitted. An arm that
declares bindings needs the walk to run after the value is checked, so that its names are
resolvable. Decide first whether the provenance walk is meant to grow these shapes or whether
the refusal is the intended boundary.

## BUG-038: a `string` passed by value to a closure, or returned by one, is released by nobody

- **Status**: open, confirmed
- **Area**: `llvm-backend`; `codegen/closures.rs` and the call-boundary summary in
  `codegen/string_ownership.rs`
- **Severity**: major. An unbounded leak, one buffer per call, on the indirect call path.

The whole-program summary that decides who releases a `string` argument is keyed by the name a
call site resolves to, and it deliberately excludes any name a local binding has taken over,
because such a call goes through the indirect path where the callee is a value rather than a
declaration. A closure is always that shape. So no closure parameter is ever recorded as
read-only, every by-value argument to one is treated as retained, the caller clears its own
drop flag at the move, and the closure's frame, which has no drop entry for its parameters,
takes nothing with it.

**Minimal repro**

```neuro
func main() -> i32 {
    val size = |z: string| -> u64 { z.len() }
    mut i: u32 = 0
    mut n: u64 = 0
    while i < 200000 {
        val s = "one" + "two"
        n = n + size(s)
        i = i + 1
    }
    if n != 1200000 { return 91 }
    0
}
```

Expected: the heap stays flat, as it does for the identical program written with a top-level
`func size(s: string) -> u64`, whose argument the caller now releases. Observed: the arithmetic
is right and the resident set climbs linearly: the loop above peaks near 7 MB resident,
against under 2 MB for the `func` spelling. The pipeline spelling of the same call,
`s |> (|z: string| -> u64 { z.len() })`, leaks identically: `|>` desugars to this call
and adds nothing of its own.

**Root cause**: confirmed in the code. `analyze` walks `HirItem` function bodies and keys both
summaries by function name; `shadowed_names` then removes every name a local binding holds.
A closure has no entry to begin with, so `param_is_read_only` answers `false` for it, which is
the answer that means "the callee may have retained this".

**The return half.** The return summary is keyed the same way, so an owned `string` a closure
RETURNS is never recognised as the caller's either. Each call below leaks its result, while the
same loop calling a top-level `func describe(n: i32) -> string { "total {n}" }` directly, or
piping into it (`i |> describe`), releases every buffer:

```neuro
func describe(n: i32) -> string { "total {n}" }
func keep(s: string) -> string { s + "" }

func main() -> i32 {
    val f = |n: i32| -> string { "total {n}" }
    val twice = describe >> keep
    mut i = 0
    mut t: u64 = 0
    while i < 10 {
        val a = f(i)          // leaks
        val b = i |> f        // leaks
        val c = twice(i)      // leaks: a composition is a closure
        t = t + a.len() + b.len() + c.len()
        i = i + 1
    }
    if t != 210 { return 1 }
    0
}
```

**Workaround**: give the stage a top-level `func` instead of a closure where it takes or returns
an owned `string`, or pass a borrow (`&string`), which is not a move and leaves the caller's flag
alone.

**Fix sketch**: the summary has to be keyed by something a closure has. Lowering gives each
closure literal a generated name for its lifted body, so the cheap version is to record that
body in the same walk under that name and have the indirect call path look it up when the
callee is a binding whose initializer is a closure literal in scope. That covers the common
case above and leaves a closure reached through a parameter or a struct field unprovable, which
is the correct conservative answer for those. The same lookup answers the return half, once
the lifted body is also entered in the return summary under its generated name. The reason this is filed rather than fixed: it
widens the summary from "top-level functions" to "every lowered body", and the indirect call
path in `codegen_call_dispatch` has to carry enough of the callee's identity to key on, which
is a change to what that path passes rather than a new arm in it. Regression tests want the
repro above, the `|>` spelling, a closure stored in a struct field (must stay conservative),
and a closure that DOES retain its argument, which must keep the current transfer.

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

## BUG-026: a later binding may not reuse a name in the same block

- **Status**: open, confirmed
- **Area**: `semantic-analysis` (scope resolution)
- **Severity**: minor. The compiler rejects; it never miscompiles, and renaming works

The language reference says a later `val` or `mut` in the same block may reuse an earlier
binding's name, shadowing it for the rest of the scope, and that shadowing may change the
type. The checker rejects the second declaration instead.

**Minimal repro**

```neuro
func main() -> i32 {
    val s = "text"
    val s = 7
    return s
}
```

Expected: compiles, `s` is `i32` and the program returns 7. Observed:

```text
variable 's' already defined in this scope
```

Shadowing across *nested* blocks is unaffected: an inner block may reuse an outer name, and
does the right thing. Only a second declaration at the same nesting level is refused.

**Root cause**: not yet confirmed in the code. The scope resolver treats a re-declaration in
one scope as a redefinition error rather than as a new binding that displaces the old name.

**Workaround**: give the second binding a different name.

**Fix sketch**: let a declaration in an occupied scope slot replace the entry rather than
report. Two things have to come with it, and they are the reason this is not a one-line
change. The reference requires the shadowed value to be dropped at the *normal end of scope*
rather than early, so the displaced binding stays registered for destruction and both are
released when the block ends. And the borrow and move checkers key on the name, so a moved
binding that is then shadowed must not report a use-after-move against the new one. The borrow
half is the harder one: a borrow records the place it borrows by name, so a `val r = &x` taken
before a second `val x` would be released against the new `x` when `r` dies, which can clear a
live borrow of the new binding and let it be moved while that borrow still reads it. A
regression test needs all three: the type change above, a shadowed `Vec` (both buffers
freed, exactly once), and a shadow of a moved binding.

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
