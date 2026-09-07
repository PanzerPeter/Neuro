# Known Bugs

Open defects only, newest first. Every confirmed bug that is not yet fixed has a numbered
`BUG-NNN` entry here; when a bug is fixed its entry is **deleted**, the fix lives in
`CHANGELOG.md`, in the affected slice's `CONTEXT.md`, and in its regression test. IDs are
never reused, so numbering stays stable as entries are removed.

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
