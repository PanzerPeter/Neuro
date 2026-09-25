# Known Bugs

Open defects only, newest first. Every confirmed bug that is not yet fixed has a numbered
`BUG-NNN` entry here; when a bug is fixed its entry is **deleted**, the fix lives in
`CHANGELOG.md`, in the affected slice's `CONTEXT.md`, and in its regression test. IDs are
never reused, so numbering stays stable as entries are removed.

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

## BUG-047: a `Drop` value that is never bound is never destroyed

- **Status**: open, confirmed
- **Area**: `llvm-backend`; drop scheduling for expression temporaries
- **Severity**: major. A destructor with a side effect (closing a handle, releasing a
  resource) silently never runs

**Minimal repro**

```neuro
struct Tok { id: i32 }

impl Drop for Tok {
    func drop(&mut self) { println("drop {self.id}") }
}

func make() -> Tok { Tok { id: 22 } }

func main() -> i32 {
    make()
    val a = make().id
    val b = Tok { id: 5 }.id
    val t = make()
    val c = t.id
    println("end")
    a + b + c - 49
}
```

Expected: every `Tok` is destroyed exactly once, as the ownership rules require. Observed:
only `t` is. The output is `end` then `drop 22`, once. The discarded `make()`, the temporary
whose field `a` reads, and the struct literal whose field `b` reads are never dropped. Binding
the value first (`val t = make()` then `t.id`) is the only form that runs the destructor.
Passing a temporary by value to a function (`consume(make())`) does drop it, inside the callee.

**Root cause**: not yet confirmed in the code. Drop flags are registered for bindings and for
the positions a value is stored into; a temporary that is read from and then discarded has
neither, so no scope exit reaches it.

**Workaround**: bind the value to a `val` before reading from it.

**Fix sketch**: give an unbound `Drop` temporary an owner for the rest of its statement and
drop it at the statement's end, after the read. Regression tests want an expression
statement, a field read, a method-call receiver and a struct literal receiver.

## BUG-039 — a function that hands back its own `string` parameter leaks the buffer

- **Status**: open, confirmed
- **Area**: `llvm-backend`; the return summary in `codegen/string_ownership.rs`
- **Severity**: major. An unbounded leak, one buffer per call, in a shape as ordinary as an
  identity or a passthrough wrapper

A call is treated as handing its caller an owned buffer only when every one of the callee's
exits is an *allocating expression* (`+`, an interpolation, `String::to_string`, or another
such call). An exit that returns a `string` **parameter** is none of those, so the caller does
not register the binding it initializes as an owner. The argument was moved into the callee at
the call, which clears the caller's own flag for it, so both ends stand down.

**Minimal repro**

```neuro
func keep(s: string) -> string { s }

func main() -> i32 {
    mut i: u32 = 0
    mut n: u64 = 0
    while i < 200000 {
        val s = "one" + "two"
        val r = keep(s)
        n = n + r.len()
        i = i + 1
    }
    if n != 1200000 { return 91 }
    0
}
```

Expected: the heap stays flat. The language makes a function's return value a storing
position, so
`r` owns the buffer and releases it at scope exit. Observed: the arithmetic is right and the
process's resident set climbs linearly, about one 6-byte buffer plus its allocator header per
iteration: the loop above peaks near 6 MB resident, against under 2 MB for the same loop with
the workaround below. Raising the round count raises the peak in step.

**Root cause**: confirmed in the code. `allocates` recognises the expression shapes that build
a buffer and nothing else; a bare `Variable` exit is not one, so `keep` never enters
`returns_owned`. The pass documents this direction as deliberate ("an unprovable case is
`false` and leaks one buffer, because the other direction frees `.rodata` or dangles"), and
for an opaque exit that is the right call. A parameter is not opaque: which buffer it names is
exactly what the caller knows.

**Workaround**: rebuild rather than forward the value (`func keep(s: string) -> string { s + "" }`),
which makes the exit an allocating shape and re-arms the caller's binding.

**Fix sketch**: the summary needs a third answer beside "allocates" and "unknown": *forwards
parameter i*. An exit that is a bare parameter read gives the caller a buffer whose ownership
it can settle itself: the argument it passed at that index. The call site then keeps its
own flag for that argument instead of transferring it, the way the read-only-parameter case
now does, rather than arming a fresh owner on the result. Both halves are a fixpoint over the
same body walk the pass already runs. Regression tests want the identity above, a conditional
forward (one exit a parameter, one an allocation, which must stay conservative), and a forward
through two calls, so a wrong transfer would double-free rather than merely leak.

## BUG-038 — a `string` passed by value to a closure, or returned by one, is released by nobody

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

## BUG-035 — a borrow reaching a binding through a call return is not tracked

- **Status**: open, confirmed
- **Area**: `semantic-analysis` (borrow checking); `borrow_target_of` in
  `type_checkers/statements.rs`
- **Severity**: major — memory-unsafe. The borrowee rules accept a program that leaves a
  reference pointing into a freed buffer, and the compiler says nothing.

A borrow becomes a tracked *persistent* borrow only when the initializer is syntactically a
borrow of a named place (`val r = &x`, or a `.slice(range)` view). A borrow that reaches the
binding any other way — most commonly as the return value of a function that takes one and
hands it back — attaches to nothing. The borrowee rules read those tracked counts, so for such
a binding they see no live borrow and every one of them stands down.

**Minimal repro**

```neuro
func id(s: &string) -> &string { s }
func consume(s: string) -> u64 { s.len() }

func main() -> i32 {
    val s: string = "hello"
    val b: &string = id(&s)
    val n: u64 = consume(s)
    return b.len() as i32
}
```

Expected: rejected with `cannot move out of 's' while it is borrowed`. Observed: type checking
passes, `s` is moved into `consume`, and `b.len()` then reads the buffer `consume` released.

The direct spelling of the same program is correctly rejected, which isolates the trigger:
replace `id(&s)` with `&s` and the diagnostic fires. The read half escapes the same way —
`val r: &mut i32 = pick(&mut n); val read: i32 = n` compiles, where the direct `&mut n` form
does not.

**Root cause**: confirmed in the code, and recorded as a known property of the pass in
`compiler/semantic-analysis/CONTEXT.md` ("only direct-borrow initializers create tracked
persistent borrows"). Before the borrowee rules existed, missing such a borrow cost only an
exclusivity diagnostic between two borrows; it now costs a dangling-pointer diagnostic, which
is what promotes the known conservatism to a defect.

**Workaround**: bind the borrow directly (`val b: &string = &s`) where the borrowee must stay
frozen. There is no workaround that keeps the indirect spelling.

**Fix sketch**: the borrow must be carried by the *type*, not recovered from the initializer's
syntax. A reference-typed binding whose initializer is a call needs the callee's elided output
lifetime resolved to the argument it came from — the same input-to-output mapping
`check_returned_reference` already relies on via `current_fn_outliving` — and then
`attach_borrow` against that argument's root place. Ranking the whole-function approach: this is
the point where per-binding counters stop paying for themselves and a borrow set keyed by
(place, region) starts to. Regression tests want the repro above, the `&mut` read variant, and a
callee returning a reference derived from `self`.

## BUG-034 — a local shadowing a labelled top-level `func` is checked against the function

- **Status**: open, confirmed
- **Area**: `argument-binding` (call-site signature table); the rest of the compiler already
  honours the shadow
- **Severity**: minor — the compiler rejects a valid program; it never miscompiles, and
  renaming the local works

A `val` binding may shadow a top-level `func` of the same name, and the type checker and the
backend both honour it: the call reaches the local and the program runs. Argument binding runs
before type checking and resolves a called name in its table of top-level functions alone. A
function enters that table only if it declares an external parameter label, so the defect needs
both halves: the shadowed function must be labelled, and its arity must differ from the local's.
Then the call is validated against the *function's* signature and rejected.

**Minimal repro**

```neuro
func scale(factor: i32, by amount: i32) -> i32 { factor * amount }

func main() -> i32 {
    val scale = |a: i32| -> i32 { a + 1 }
    return scale(5)
}
```

Expected: compiles and returns 6. Observed, before type checking runs at all:

```
Argument errors found:
  1. 'scale' takes 2 argument(s), but 1 given
```

Two neighbouring programs show the shadow itself is supported and isolate the trigger. Drop the
label (`func scale(value: i32)`) and the same shadowing program compiles and exits 6, because an
unlabelled function never enters the table. Give the local the *same* arity as the labelled
function and `scale(5, by: 2)` compiles and exits 7 — the label is checked against the function
and then bound positionally to the closure, which takes no labels at all.

**Root cause**: confirmed in the code, and already written down as a known property of the pass
in `compiler/argument-binding/CONTEXT.md` ("Local bindings are not tracked"). The walk that
validates calls looks a bare callee up in the table of labelled top-level declarations; nothing
in it knows which names a local binding has taken over in the enclosing scope.

**Workaround**: rename the local, or call the shadowed function with its own arity.

**Fix sketch**: the pass needs a scope stack, not a flat table — push a frame per block, record
every `val` / `mut` / closure parameter name in it, and skip label validation for a callee whose
name a live local holds. That also closes the third shape above, where a label survives onto a
closure call that cannot take one. It is the same scope walk module resolution needs for its own
shadowing gap (it rewrites an imported name whether or not a local covers it), so the two are
worth taking together rather than each growing a private half-version. Regression tests want the
repro above, the same-arity labelled call, and a call placed after the local goes out of scope,
which must still reach the function.

## BUG-033 — `&` does not accept a field or an element, only a bare variable

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

```
cannot borrow this expression: `&` requires a place (a variable); bind it to a `val` first
```

Dropping the `&` swaps one error for the other:

```
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

## BUG-031 — `.step(n)` on a range is specified but has no implementation and no checkbox

- **Status**: open, confirmed
- **Area**: `semantic-analysis` (range method dispatch); reaches `syntax-parsing`,
  `hir-lowering` and `llvm-backend` once implemented
- **Severity**: major. A documented construct of a closed sub-phase does not compile.

The specification gives `.step(n)` as a method on any range, in three places: as a
`for`-head adapter, in the list of adapters ranges implement alongside `.enumerate()` /
`.map()` / `.filter()`, and as a tensor index (`tensor[(0..n).step(2)]`). The syntax
summary lists both the range form and the tensor step-slice form. No range method named `step` exists in the
compiler, so all of those spellings are rejected.

**Minimal repro**

```neuro
func main() -> i32 {
    mut t = 0
    for i in (0..6).step(2) { t = t + i }
    t
}
```

Expected: `6` (0 + 2 + 4). Observed: a compile error reporting that a range expression is
only valid as the argument to `.slice()` or `.char_slice()`, followed by a cascaded
`undefined variable 'i'`. The tensor-index form
(`val s: Tensor<i32, [3]> = a[(0..6).step(2)]`) fails the same way.

**Root cause**: there is no `step` method on a range at all. A range expression is accepted
only in a `for` head and as the argument to `.slice()` / `.char_slice()`; any method call on
one falls through to the diagnostic above. The diagnostic is accurate about what the checker
supports and silent about the fact that the language defines the method.

**Why this is filed rather than scheduled**: the roadmap item that adds `.rev()` to ranges
has since shipped, and it named only `.rev()`. The `for`-head form of `.step(n)` belongs to
a sub-phase that closed long before that, and the tensor-index half was deferred in the
internal notes archive to an item whose text never grew to cover it. The roadmap's own
spec-coverage rule says a deferral written in the prose of a closed item is not tracking,
and that every construct a spec section names needs either an implementation or a checkbox
of its own. `.step(n)` has neither.

**Workaround**: write the stride into the loop body or the index arithmetic
(`for i in 0..3 { val j = i * 2 ... }`).

**Fix sketch**: feature-sized, not a surgical fix, and it wants a checkbox before it is
worked. Which line it goes on is a scheduling decision; a 2B line of its own, directly
after the shipped `.rev()` one, is the natural place. Most of the shape is already there:
`.rev()` is peeled in the parser as an innermost range form, ridden through as a flag on
`Stmt::ForRange` and on `TensorIndexArg::Range`, and honoured by the counted-loop lowering
and the tensor slice path. `.step(n)` is the same route with a stride instead of a flag,
and the two compose (`.rev().step(n)`).

## BUG-030 — an element moved out of a `Vec` leaves the `Vec` owning it too

- **Status**: open, confirmed
- **Area**: `semantic-analysis` (move analysis of index places)
- **Severity**: major — two owners of one heap buffer; not yet observable as a crash only
  because an anonymous heap string is never freed today

Move analysis records a move out of a binding and, since the struct-field fix, out of a
field place. An **index** place is still not a place it recognises, so binding an element
of a `Vec<string>` moves nothing: the element and the `Vec` both own the same buffer, and
a `Vec` frees its elements on `Drop`.

**Minimal repro**

```neuro
func main() -> i32 {
    mut v: Vec<string> = Vec::new()
    v.push("a" + "b")
    val x = v[0]
    return (x.len() as i32) + (v[0].len() as i32)
}
```

Expected: a diagnostic, the way the same program written against a plain binding or a
struct field gets one. Observed: it compiles, and `x` and `v` own one buffer between them.
It exits 4 rather than crashing because an owned string built by `+` is never freed — the
untracked leak Phase 1 left behind — so the double free has nothing to fire on yet. A
collection of a type with a real destructor would abort.

**Root cause**: `record_move` resolves a place through `place_origin`, which now handles an
index place over a fixed ARRAY but still returns `None` for one over a `Type::Collection`,
so no move is recorded and no error is raised.

**Workaround**: read the element through a method or a loop over the collection rather
than binding it, or `.clone()` it.

**Fix sketch**: not purely mechanical, which is why it is filed rather than fixed. An array
and a tuple now answer this per ELEMENT PATH — `a[0]` moves the path `"0"`, a runtime `a[i]`
moves the whole binding, because the compiler cannot say which element left. A `Vec`'s
length is not static, so every index into one is the runtime case, and applying the array
rule unchanged would make a `Vec` of a non-`Copy` element readable exactly once — `&v[0]`
is not a borrowable place either. What a partial move of a collection means is still a
language decision the spec does not make. Decide the rule first (reject the move outright,
as Rust does; require `.clone()`; or add a borrowing index form), then implement it.

## BUG-027 — a const generic parameter cannot be passed to another generic call

- **Status**: open, confirmed
- **Area**: `semantic-analysis` (generic inference, `unify_array_len` / `seed_turbofish`)
- **Severity**: major — a generic function cannot delegate to another over its own const
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

```
generic parameter 'N' cannot be inferred from the call arguments;
supply it explicitly with a turbofish, e.g. `f::<...>(...)`
```

The turbofish the message recommends does not work either. `sum_n::<N>(t)` reports

```
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
rather than patched. A substitution has to be able to hold a *symbolic* const — "the
caller's parameter `N`" — not only a `Type::ConstValue`; `unify_array_len` then binds the
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

## BUG-026 — a later binding may not reuse a name in the same block

- **Status**: open, confirmed
- **Area**: `semantic-analysis` (scope resolution)
- **Severity**: minor — the compiler rejects; it never miscompiles, and renaming works

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

```
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
binding that is then shadowed must not report a use-after-move against the new one. A
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
