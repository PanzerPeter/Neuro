# Known Bugs

Open defects only, newest first. Every confirmed bug that is not yet fixed has a numbered
`BUG-NNN` entry here; when a bug is fixed its entry is **deleted**, the fix lives in
`CHANGELOG.md`, in the affected slice's `CONTEXT.md`, and in its regression test. IDs are
never reused, so numbering stays stable as entries are removed.

## BUG-017 — a triple-quoted string keeps the newline before its closing delimiter

- **Status**: open, not yet fixed. Needs a maintainer ruling first: the fix changes the
  value of every block string in the language.
- **Area**: lexical analysis (block-string body assembly)
- **Severity**: minor — the value is wrong by one trailing byte, consistently and
  predictably; nothing crashes and nothing is silently mis-rendered.
- **Repro**:

  ```neuro
  func main() -> i32 {
      val block = """
          ab
          """
      return block.len() as i32   // returns 3; the specification says 2
  }
  ```

- **Expected**: `"ab"` — the language specification states that the newline right after the
  opening delimiter is dropped *and* that the newline before the closing delimiter's line is
  dropped as well, so a three-line block is three lines "with no leading or trailing blank".
- **Observed**: `"ab\n"`. Printing a block with `println` therefore emits a blank line after it.
- **Root cause**: the body assembler drops the opening newline only. The closing line's own
  newline survives into the value.
- **Why it is not fixed here**: the current behaviour is deliberately encoded in committed
  artifacts — an example program asserts `"first\nsecond\n"` in six separate checks, two
  showcase programs and their recorded output depend on it, and several lexer unit tests
  spell the trailing `\n` out. Changing it alters the observable text of every existing
  block string, which is a language-surface decision rather than a patch.
- **Workaround**: none needed for the common case — most uses print the block, where the
  extra newline merges with `println`'s own. Where the exact value matters, take
  `block.slice(0..block.len() - 1)`.
- **Fix sketch**: drop the final newline when assembling the block body, then update
  `examples/types/triple_quoted.nr`, the two showcase programs and their recorded output if
  their text shifts, and the lexer tests that name the trailing `\n`. Alternatively, if the
  retained newline is the intended design (it matches how a heredoc behaves elsewhere),
  correct the specification instead — but the two must agree.

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
