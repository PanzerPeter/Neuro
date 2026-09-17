# Known Bugs

Open defects only, newest first. Every confirmed bug that is not yet fixed has a numbered
`BUG-NNN` entry here; when a bug is fixed its entry is **deleted**, the fix lives in
`CHANGELOG.md`, in the affected slice's `CONTEXT.md`, and in its regression test. IDs are
never reused, so numbering stays stable as entries are removed.

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
- **Area**: `semantic-analysis` (borrow checking); the same missing place-expression
  machinery BUG-025 describes for assignment targets, on the rvalue side
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

**Why this is filed rather than left to the sub-phase that covers it**: the value model
sub-phase carries "place expressions" as an item, written against assignment targets
(`t[i, j] = v`, `self.x += dx`), which is BUG-025's half. The rvalue borrow is not named
there and is the half a layer type needs first. Whoever takes the place expression on
should take both; this entry is here so the second half is not lost.

**Fix sketch**: give the borrow check the same notion of a place that plain assignment
already resolves — identifier, field access, index — and carry the resolved place through
to codegen, which must produce the address of the field or element rather than of a named
slot. Worth confirming against BUG-025 before starting: the two share the representation
and are cheaper together than apart.

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

**Root cause**: `record_move` resolves a place through `place_origin`, which handles an
identifier, a field access, and a dereference. It returns `None` for `Expr::Index`, so no
move is recorded and no error is raised.

**Workaround**: read the element through a method or a loop over the collection rather
than binding it, or `.clone()` it.

**Fix sketch**: not purely mechanical, which is why it is filed rather than fixed. The
conservative rule that works for a field — mark the ROOT binding moved — would make a
`Vec` of a non-`Copy` element readable exactly once, since `&v[0]` is not a borrowable
place either. What a partial move of a collection means is a language decision the spec
does not make: fixed arrays and tuples sidestep it by rejecting non-`Copy` elements
outright, and a `Vec` does not. Decide the rule first (reject the move outright, as Rust
does; require `.clone()`; or add a borrowing index form), then implement it.

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

## BUG-025 — compound assignment only accepts a plain variable as its target

- **Status**: open, confirmed
- **Area**: `syntax-parsing` (statement parser)
- **Severity**: major — a construct the language reference uses in its own canonical
  example does not parse

`x += 1` parses only when `x` is a bare identifier. A field, an element, or anything else
assignable is a parse error, even though plain assignment accepts all of them.

**Minimal repro**

```neuro
struct Point { x: i32, y: i32 }

impl Point {
    func translate(&mut self, dx: i32, dy: i32) {
        self.x += dx
        self.y += dy
    }
}

func main() -> i32 {
    mut q = Point { x: 3, y: 4 }
    q.translate(1, 2)
    return q.x * 10 + q.y
}
```

Expected: compiles and returns 46. Observed:

```
failed to parse module: unexpected token PlusEqual, expected expression
```

The same error covers `arr[i] += 5` and, on the tensor side, `layer.weights -= grad`. Plain
assignment to either place (`self.x = ...`, `arr[i] = ...`) parses and runs correctly, so the
gap is specific to the compound forms. The message is also unhelpful: it names the operator
token rather than saying that a compound assignment needs a variable on the left.

**Root cause**: not yet confirmed in the code. The statement parser commits to a compound
assignment only after reading a bare identifier, so a target that begins as a field or index
expression falls through to the expression parser, which then meets `+=` with nothing to do.

**Workaround**: write the desugaring by hand, `self.x = self.x + dx`. It is exactly
equivalent for scalars. It is *not* equivalent for a tensor, where the whole point of the
compound operator is that it updates the existing buffer instead of allocating a new one, so
there is no workaround for a tensor held in a struct field.

**Fix sketch**: parse the target as a place expression (identifier, field access, or index)
the way plain assignment already does, and carry it on the compound-assignment statement.
Both consumers re-form the by-value desugaring themselves for the types that take it, so each
has to handle a place target rather than a name; the tensor path in the checker and in
codegen resolves the target's storage instead of looking a name up. Worth splitting: making
the parse error actionable is small and independent, and is worth doing even if the parser
keeps refusing the form.

**Note for whoever takes this on**: field-target compound assignment on tensors is also
listed as out of scope for the sub-phase that shipped in-place tensor assignment, so confirm
the intended scope on the issue before writing the tensor half.

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
