# Known Bugs

Open defects only, newest first. Every confirmed bug that is not yet fixed has a numbered
`BUG-NNN` entry here; when a bug is fixed its entry is **deleted**, the fix lives in
`CHANGELOG.md`, in the affected slice's `CONTEXT.md`, and in its regression test. IDs are
never reused, so numbering stays stable as entries are removed.

## BUG-028 — an annotation's type does not reach a `break` value

- **Status**: open, confirmed
- **Area**: `semantic-analysis` (expected-type propagation)
- **Severity**: minor — the compiler rejects a program the equivalent `if`, `match`, and
  block forms all accept; it never miscompiles

An expected type flows into an `if` arm, a `match` arm, a bare block's tail, and a
function's `return`, so a tensor literal written in any of those coerces against the
annotation. It does not flow into the value of a `break`, so the same literal in a value
loop is typed as a plain array and then fails to match.

**Minimal repro**

```neuro
func main() -> i32 {
    mut i = 0
    val t: Tensor<i32, [2]> = loop {
        i = i + 1
        if i == 1 { break [10, 20] }
    }
    return t[0] + t[1]
}
```

Expected: compiles and returns 30, the way every other form of the same program does.
Observed:

```
type mismatch: expected Tensor<i32, [2]>, found [i32; 2]
```

These three are accepted, which is what makes the `break` case a defect rather than a
missing feature: `val t: Tensor<i32, [2]> = if c { [1, 2] } else { [3, 4] }`, the same with
`match`, and the same with a bare block. An explicit constructor in the `break`
(`break Tensor::<i32, [2]>::ones()`) is also accepted, so only the literal coercion is
affected.

**Root cause**: `check_loop_expr` takes the expected type but passes it no further than its
own fallback for a loop with no `break`; `check_loop_body` never receives it, so `LoopContext`
carries no expected type and the `Stmt::Break` arm checks its value with no annotation to
coerce against.

**Workaround**: write the constructor instead of the literal, or bind the literal to an
annotated `val` inside the loop and `break` that.

**Fix sketch**: thread the expected type through `check_loop_body` into `LoopContext`, and
have the `Stmt::Break` arm pass it to `check_expr` as the expected type of the break value.
The agreement check between several `break`s in one loop stays as it is. A regression test
wants the repro above plus a loop whose `break`s disagree, so the added expectation does not
mask a genuine mismatch.

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
