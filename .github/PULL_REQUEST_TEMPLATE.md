## What this changes

<!-- What problem does this solve, and what approach did you take? Link the issue it
     closes: "Closes #123". A bug fix should name its BUG-NNN id. -->

## Tests

<!-- What you added and why it gives confidence. A bug fix needs a regression test that
     fails without the change: say which test that is. -->

## Breaking changes

<!-- Syntax, inference behaviour, CLI flags, the ABI of compiled code. Write "none" if
     there are none. -->

## Checklist

- [ ] `cargo test --workspace` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` is clean
- [ ] `cargo fmt --all` applied
- [ ] `python tools/check_docs_hygiene.py` passes
- [ ] `CHANGELOG.md` updated, if this is user-facing
- [ ] Each touched slice's `CONTEXT.md` updated in this same PR, if its entry point, public surface, or shared-kernel dependency moved
- [ ] No new dependency between feature slices
- [ ] No `unwrap()` or `expect()` on a production path, and every `unsafe` block carries a safety rationale
- [ ] Commit subjects use a known scope (`lexer`, `parser`, `semantic`, `codegen`, `infra`, `tests`, `docs`, `build`, `ci`) and stay under 50 characters
- [ ] Every commit carries a `Signed-off-by:` trailer (`git commit -s`)
- [ ] Branch is up to date with `main`

If this touches the lexer, one more:

- [ ] `neuro-language-support/syntaxes/neuro.tmLanguage.json` updated by hand, and `cargo test -p lexical-analysis --test tmlanguage_sync` passes

<!-- Unsigned commits will not be merged. The DCO check runs in CI, and signing off
     accepts the relicensing terms in the LICENSE. Amend with
     `git commit --amend --signoff`, or `git rebase --signoff <base>` for a series. -->
