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

## BUG-013, using a function name as a value fails with a misleading diagnostic

- **Status**: OPEN, good first task (diagnostic-sized; no language change required)
- **Area**: semantic-analysis (diagnostic quality)
- **Severity**: minor, loud error, wrong explanation
- **Repro**:

  ```neuro
  func apply_twice(f: (i32) -> i32, x: i32) -> i32 { f(f(x)) }
  func inc(x: i32) -> i32 { x + 3 }

  func main() -> i32 {
      apply_twice(inc, 10)
  }
  ```

  The compiler reports `undefined variable 'inc'`. That message claims the name does not
  exist, but it does; it just is not usable *as a value*.
- **Root cause**: functions live in a separate namespace from variables and there is no
  coercion from a function item to a function-typed value, so resolving `inc` in value
  position falls through to the ordinary variable lookup and fails with the generic
  undefined-variable diagnostic.
- **What should happen**: either (a) a dedicated diagnostic, "`inc` is a function;
  functions are not first-class values, wrap it: `|x: i32| inc(x)`", which is purely
  corrective, or (b) minimal fn-item-to-fn-pointer coercion so the call compiles. Option
  (a) is the right first step and needs a maintainer decision only if (b) is wanted later.
- **Workaround**: wrap the call in a closure, `apply_twice(|x: i32| inc(x), 10)`.

---

## BUG-012, a bare `return` as a `match` arm body does not parse

- **Status**: OPEN, needs a maintainer decision before code changes (spec conflict)
- **Area**: syntax-parsing (`parse_match_arm`)
- **Severity**: minor, clean parse error, easy workaround, but the specification itself
  disagrees with the grammar
- **Repro**:

  ```neuro
  func f(n: i32) -> i32 {
      match n {
          0 => return 1,
          _ => 2
      }
  }
  ```

  This fails with `unexpected token Return, expected expression`. The braced form
  `0 => { return 1 }` parses, type-checks, and runs correctly.
- **Root cause**: `parse_match_arm` parses the arm body as an expression, and `return`
  is a statement, not an expression. A braced body reaches the parser as a block, which
  holds statements, hence the asymmetry.
- **The conflict**: the specification's own `val-else` example writes bare `return` arm
  bodies, so the spec is internally inconsistent: the grammar notes list `return` under
  statements, while the example uses it in expression position.
- **Two ways to resolve (both cheap; the decision is which one is the language)**:
  1. **Parser accepts it.** In `parse_match_arm`, when the body starts with `return`,
     `break`, or `continue`, parse that statement and wrap it in a single-statement
     block. Purely additive: no existing program changes meaning, and the current
     arm-divergence rule already types such an arm correctly. Roughly ten lines in one
     function. This makes the spec's example compile as written.
  2. **Spec adds the braces.** Fix the example to use `{ return ... }` and leave the
     grammar alone, keeping the statement/expression split absolute.
- **Recommendation**: option 1. The braces carry no information, the shape is idiomatic
  in every language with `match`, and the spec already reaches for it unprompted.
- **Workaround**: brace the arm body, `0 => { return 1 }`.
