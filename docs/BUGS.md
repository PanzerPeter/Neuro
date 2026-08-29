# Known Bugs

Open defects only, newest first. Every confirmed bug that is not yet fixed has a numbered
`BUG-NNN` entry here; when a bug is fixed its entry is **deleted**, the fix lives in
`CHANGELOG.md`, in the affected slice's `CONTEXT.md`, and in its regression test. IDs are
never reused, so numbering stays stable as entries are removed.

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

---

## BUG-015, `==` on a struct with no `PartialEq` implementation crashes the backend

- **Status**: OPEN
- **Area**: semantic-analysis (the missing rejection), with the crash surfacing in
  llvm-backend
- **Severity**: major — a program the type checker accepts kills the compiler with an
  internal error instead of producing a diagnostic or a binary
- **Repro**:

  ```neuro
  struct V { x: i32 }

  func main() -> i32 {
      val a = V { x: 1 }
      val b = V { x: 2 }
      if a == b { return 1 }
      return 0
  }
  ```

  `neurc check` reports **no error**. `neurc compile` then aborts:

  ```
  Found StructValue(StructValue { struct_value: Value { name: "a1", ...
    llvm_type: "{ i32 }" } }) but expected the IntValue variant
  ```

- **Root cause**: the equality path assumes its operands are comparable primitives and
  reaches codegen without ever checking that the struct implements `PartialEq`. The
  backend then asks a struct value for its `IntValue` variant and panics on the
  mismatch. The correct path works — `@derive(Copy, Clone)` plus an explicit
  `impl PartialEq for V { func eq(...) func ne(...) }` compiles and runs — so this is a
  missing rejection on the *unimplemented* path, not a broken operator.
- **What should happen**: `a == b` on a struct with no `PartialEq` implementation is a
  type error naming the struct and the missing trait, reported by semantic-analysis. An
  `impl PartialEq` for a non-`Copy` struct is already rejected with a clear message
  (`operator trait 'PartialEq' can only be implemented for a `Copy` type`), so the
  diagnostic vocabulary exists.
- **Related**: `@derive(PartialEq)` — and `@derive(Bogus)` — are currently accepted and
  silently ignored, since only `Copy` and `Clone` are implemented. A program can
  therefore write the derive the specification documents, get no error, and hit this
  crash. Validating `@derive` arguments is tracked separately on the roadmap; it does
  not fix this entry on its own.
- **Workaround**: add `@derive(Copy, Clone)` to the struct and implement `PartialEq`
  explicitly, or compare the fields.

---

## BUG-014, a named argument is evaluated in declaration order, not source order

- **Status**: OPEN, needs a maintainer decision before code changes (the language does
  not currently define argument evaluation order)
- **Area**: argument-binding (`bind`), with the observable consequence in codegen
- **Severity**: minor today, latent major — two call forms the specification calls
  equivalent do not evaluate their arguments in the same order
- **Repro**:

  ```neuro
  func fail_a() -> i32 { panic("EVAL-A") }
  func fail_b() -> i32 { panic("EVAL-B") }
  func combine(first: i32, second: i32) -> i32 { first + second }

  func main() -> i32 { combine(second: fail_b(), first: fail_a()) }
  ```

  This panics with `EVAL-A` — the argument written *second* runs *first*. The
  all-positional control `combine(fail_b(), fail_a())` panics with `EVAL-B`, so
  positional arguments do evaluate left to right; only the named form differs.
- **Root cause**: `bind` permutes the `args` vector into the callee's declaration
  order and drops the labels, so by the time any later stage sees the call, the
  source order of the argument *expressions* is gone. Everything downstream then
  evaluates them in the order it finds them.
- **What should happen**: the specification's own example comments a reordered named
  call as `// same` as the declaration-ordered one, which holds for the parameter
  each value binds to but not for the order the values are computed in. Every
  language with named arguments evaluates them in source order. The fix is to bind
  each argument expression to a temporary in source order at the call site and pass
  the temporaries in declaration order; that is a lowering change, and it gives up
  the current property that a named call produces byte-identical IR to the
  positional one, so it wants a ruling before it is written.
- **Why it is only minor today**: the exclusivity rule rejects two arguments that
  both mutably borrow the same place, so the difference is presently observable only
  through which of two panicking arguments fires first. It becomes an ordinary
  wrong-answer bug as soon as arguments can carry independent observable effects.
- **Workaround**: bind the arguments to `val`s in the order they must run, then pass
  the bindings by name.

---

