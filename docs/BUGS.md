# Known Bugs

Open defects only, newest first. Every confirmed bug that is not yet fixed has a numbered
`BUG-NNN` entry here; when a bug is fixed its entry is **deleted**, the fix lives in
`CHANGELOG.md`, in the affected slice's `CONTEXT.md`, and in its regression test. IDs are
never reused, so numbering stays stable as entries are removed.

## BUG-016 — binding a `void` call passes the type checker and crashes the backend

**Status:** open. **Affects:** semantic analysis (missing diagnostic).

A `val` bound to a call that returns nothing is accepted by `neurc check` and then aborts
code generation with an internal error instead of a type error.

```neuro
func nothing() {
    val a = 1
}

func main() -> i32 {
    val x = nothing()      // accepted by `check`
    return 0
}
```

```
Compilation failed: Failed to generate object code
  Caused by (1): Code generation error: internal compiler error: function call returned
  void when value expected
```

**Root cause.** Nothing rejects a binding whose initializer has type `void`. The backend's
call dispatch returns `None` for a unit call — correct, and what statement position wants —
and the value-position arm turns that into an `InternalError`, which is the right last line
of defence but the wrong place for the program to be caught.

**Scope.** Any `void`-returning callee: a user function with no return type, and the
`print` / `println` builtins, which make the shape easier to write by accident than it used
to be. Statement position (`nothing()` / `println("hi")` on its own
line) is unaffected and correct.

**Fix sketch.** Reject it in `semantic-analysis` where the binding's initializer type is
recorded (`type_checkers/statements.rs`, the `Stmt::VarDecl` arm): a `Type::Void`
initializer is a new `TypeError` naming the callee, in the same shape as the other binding
diagnostics. The backend's `InternalError` then becomes genuinely unreachable and stays as
the assertion it is. A regression test belongs in `neurc/tests/` alongside the existing
`void`-tail cases.

## Taking one of these on

Each entry below is a self-contained task: repro, root cause where known, and a fix
sketch. If you want to work on one:

1. Open (or claim) an issue naming the `BUG-NNN` id so work isn't duplicated.
2. Follow the normal [contribution workflow](../CONTRIBUTING.md), branch, tests,
   quality gates, DCO sign-off.
3. A bug fix **must** ship with a regression test that fails without the fix. Name it
   after the defect so its purpose is obvious.
4. If an entry turns out to be a language-design decision rather than a patch, say so in
   the issue, some of these need a maintainer ruling before code changes.
