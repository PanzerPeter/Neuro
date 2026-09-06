# Known Bugs

Open defects only, newest first. Every confirmed bug that is not yet fixed has a numbered
`BUG-NNN` entry here; when a bug is fixed its entry is **deleted**, the fix lives in
`CHANGELOG.md`, in the affected slice's `CONTEXT.md`, and in its regression test. IDs are
never reused, so numbering stays stable as entries are removed.

## BUG-022 — constant folding wraps silently on overflow

**Repro**

```neuro
const C: u8 = 200u8 + 100u8

func main() -> i32 {
    println("const sum = {C}")     // prints 44
    return 0
}
```

The same arithmetic in a function body aborts on the debug tier:

```neuro
func main() -> i32 {
    mut a: u8 = 200u8
    val c: u8 = a + 100u8                  // panic: integer overflow
    return 0
}
```

A `const` initializer is evaluated by the compiler, so the debug tier's overflow panic has
nowhere to fire; the folder produces a value instead, and it produces the wrapped one. The
result is that a quantity written in a `const` and the same quantity computed in a function
disagree, on the tier whose whole purpose is to make that disagreement impossible.

**Root cause** — `fold_const` in `compiler/llvm-backend/src/codegen/expressions/literals.rs`
uses `wrapping_add` / `wrapping_sub` / `wrapping_mul` / `wrapping_neg` throughout and has no
error path for overflow. Every operator is affected, not one of them.

**Workaround** — none needed for correctness if the wrap was intended; write the folded value
directly when it was not. `docs/language-reference/types.md` documents the current behaviour
("compile-time constant folding always uses wrapping arithmetic regardless of optimization
level"), so a program relying on it is not relying on an accident.

**Fix sketch** — the ruling that settles it is already made for run-time arithmetic (debug
panics, release wraps), and a constant expression has no run-time tier to defer to: a `const`
whose initializer overflows
cannot produce a defined value under the debug rule and under the release rule at once. Rejecting
it at compile time is the only answer that does not make the tier observable in a value. That
means `fold_const` returning a `Result` that carries the overflow, and a diagnostic naming the
operator and the type. Deliberately NOT bundled with the unary-negation fix that closed BUG-021:
fixing negation alone inside the folder would recreate exactly the asymmetry BUG-021 was about,
with `const N: u8 = -ONE` rejected while `const N: u8 = 0u8 - ONE` still wrapped. All operators
move together or none do. The documentation sentence above is part of the change.

## Taking one of these on

Each entry above is a self-contained task: repro, root cause where known, and a fix
sketch. If you want to work on one:

1. Open (or claim) an issue naming the `BUG-NNN` id so work isn't duplicated.
2. Follow the normal [contribution workflow](../CONTRIBUTING.md), branch, tests,
   quality gates, DCO sign-off.
3. A bug fix **must** ship with a regression test that fails without the fix. Name it
   after the defect so its purpose is obvious.
4. If an entry turns out to be a language-design decision rather than a patch, say so in
   the issue, some of these need a maintainer ruling before code changes.
