# Contributing to Neuro

Thank you for your interest in contributing to the Neuro programming language compiler.

## Project Status

A phase is divided into lettered sub-phases implemented strictly in dependency order. Per-phase
status lives in exactly one place, the [Quick Roadmap](README.md#quick-roadmap), and what each
release changed is in [CHANGELOG.md](CHANGELOG.md).

Version numbers track the roadmap, not SemVer: MAJOR is the phase, MINOR is one roadmap item, and
PATCH is a fix, a test or a docs change. Architecture and design are still evolving, so a breaking
change can land in any MINOR.

## Table of Contents

1. [Getting Started](#getting-started)
2. [Where to Start](#where-to-start)
3. [Development Workflow](#development-workflow)
4. [Coding Standards](#coding-standards)
5. [Architecture Guidelines](#architecture-guidelines)
6. [Testing Requirements](#testing-requirements)
7. [Submitting Changes](#submitting-changes)
8. [Development Tips](#development-tips)
9. [Developer Certificate of Origin (DCO)](#developer-certificate-of-origin-dco)
10. [License](#license)

## Getting Started

### Prerequisites

- **Rust**: the `rust-version` in the root [Cargo.toml](Cargo.toml) or later (`rustup update stable`)
- **LLVM 23 and MLIR 23** in one prefix, with `LLVM_SYS_231_PREFIX`, `MLIR_SYS_230_PREFIX` and
  `TABLEGEN_230_PREFIX` all pointing at it, because every build compiles the `mlir-backend` slice.
  The [installation guide](docs/getting-started/installation.md) has each platform's recipe.
- **Git**
- **IDE**: VS Code with rust-analyzer recommended

### Verify the Build

```bash
git clone https://github.com/PanzerPeter/Neuro.git
cd Neuro
cargo build --workspace
cargo test --workspace
```

### Understanding the Codebase

Before contributing, read:

1. [README.md](README.md): project overview and installation
2. [DESIGN.md](DESIGN.md): language design principles and non-goals
3. [VSA.md](VSA.md): the architecture rules the compiler is built to
4. [docs/README.md](docs/README.md): technical documentation index
5. [CHANGELOG.md](CHANGELOG.md): recent changes
6. Each slice's `CONTEXT.md`: purpose, entry point, and dependency contract

## Where to Start

### Fix a known bug

A bug fix is the highest-value first contribution. [docs/BUGS.md](docs/BUGS.md) is the register of
open, confirmed defects. Each entry carries a minimal reproduction, the known root cause, a
workaround and a fix sketch, and many are sized for a first-time compiler contributor.

1. Claim it in an issue naming the `BUG-NNN` id so work isn't duplicated.
2. Some entries are language-design decisions rather than patches. Those say so, and need a
   maintainer ruling in the issue before any code changes.
3. Ship the fix with a regression test that fails without it (see
   [Testing Requirements](#testing-requirements)) and follow the normal workflow below.

Found a new bug? Open an issue with a minimal reproducible example instead of a PR. Confirmed
defects get filed into `docs/BUGS.md` from there.

### Non-Code Contributions

- Documentation improvements and tutorials
- Bug reports with minimal reproducible examples
- Language design feedback on [GitHub Discussions](https://github.com/PanzerPeter/Neuro/discussions)

## Development Workflow

### Before You Start

1. Check open issues and discuss major changes before starting work
2. Verify your contribution fits the phase the [Quick Roadmap](README.md#quick-roadmap) marks in
   progress
3. For architectural changes, open an issue first

The issue chooser has a form for each of these: a bug report, a language or compiler feature
proposal, a claim on a `BUG-NNN` id or a Quick Roadmap goal, and a documentation problem. Pick the
one that fits; the fields are the questions triage would ask anyway.

### Working on a Feature

1. `git checkout -b feature/your-feature-name`
2. Implement following the coding and architecture standards
3. Write tests (unit + integration) for all new functionality
4. Pass all quality gates (see below)
5. Submit a pull request with a clear description

### Commit Message Format

```text
scope: short summary (50 chars or less)

Optional longer description. Explain what and why, not how.
Reference issues with #issue-number.

Signed-off-by: Your Name <your.email@example.com>
```

**Scopes**: `lexer`, `parser`, `semantic`, `codegen`, `infra`, `tests`, `docs`, `build`, `ci`

**Examples**:
- `parser: add struct definition parsing`
- `codegen: fix while-loop break target for nested loops`
- `infra: add Span display impl for error formatting`

Every commit in a pull request **must** carry a `Signed-off-by:` trailer. See
[Developer Certificate of Origin (DCO)](#developer-certificate-of-origin-dco).

## Coding Standards

### Rust Rules

- **No `unwrap()`/`expect()` in production paths.** Use `Result<T, E>` with actionable errors.
- **Explicit integer widths** on public-facing APIs (`u32`, `i64`, not `usize` for domain values)
- **`pub(crate)` by default.** Only slice entry points are `pub`.
- **Borrowing over cloning.** Avoid unnecessary `.clone()`.
- **Suppress a lint with `#[expect(...)]`, never `#[allow(...)]`.** A stale expectation warns.
- **Document public APIs** with `///` doc comments; document every `unsafe` block with a safety rationale

### Quality Gates

All PRs must pass:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                 # includes the VSA architecture tests
python tools/check_docs_hygiene.py
```

### Keep the Editor Grammar in Step with the Lexer

Nothing links `TokenKind` to the editor grammar, so any lexer change must also update
`neuro-language-support/syntaxes/neuro.tmLanguage.json` by hand in the same commit.
`cargo test -p lexical-analysis --test tmlanguage_sync` catches a missing keyword, an invented one,
and a declaration rule ordered behind the keyword rule.

## Architecture Guidelines

Neuro uses **Vertical Slice Architecture (VSA)**: each compiler feature is a self-contained crate.
[VSA.md](VSA.md) holds the rules and the [README](README.md#architecture) the crate layout. In
short:

- A feature slice depends only on infrastructure crates (`shared-types`, `ast-types`, `neuro-hir`),
  never on another feature slice. The one exception is `syntax-parsing` on `lexical-analysis`.
  Duplication is preferred over coupling.
- `neurc` is the only crate that depends on every slice.
- No business logic in infrastructure crates.
- `pub(crate)` is the default visibility. No permissive `pub use` of slice internals.
- Update a slice's `CONTEXT.md` when its entry point or dependencies change.

### Adding a New Feature Slice

1. `cargo new compiler/feature-name --lib`
2. Add it to `members` in the root `Cargo.toml`, and inherit the workspace fields and lints as the
   other slices do
3. Add it to the slice lists in `compiler/neurc/tests/architecture_tests.rs`
4. Depend only on infrastructure crates
5. Design a single public entry point, keep all internals `pub(crate)`, and call it from `neurc`
6. Write comprehensive tests
7. Add a `CONTEXT.md` with Purpose, Entry Point, Shared Kernel, and Notes sections

## Testing Requirements

All contributions must include:

- **Unit tests** for individual functions (`#[cfg(test)] mod tests { ... }` in source file)
- **Integration tests** in the crate's `tests/` directory for slice entry points
- **Edge cases:** boundary conditions and error paths
- **Regression tests** for bug fixes

### Test Organization

```bash
cargo test --workspace            # everything
cargo test -p lexical-analysis    # one slice
cargo test <test_name>            # one test by name
cargo test -- --nocapture         # show test output
```

End-to-end tests live in `compiler/neurc/tests/suite/`, one file per language feature, all compiled
into the single `suite` test target. A new file goes there and gets a `mod` line in
`suite/main.rs`: Cargo links one executable per `tests/*.rs`, so a file added at the `tests/` root
costs a whole extra binary on every build. Example programs are pinned by their exit code and
output; [examples/README.md](examples/README.md#adding-an-example) has the steps.

### Test Quality

- Tests must be isolated, with no shared mutable state between them
- Tests must be deterministic, so the same input always produces the same result
- Test names must be descriptive and self-documenting
- Avoid testing implementation details; test observable behavior

## Submitting Changes

Opening a pull request loads [the PR template](.github/PULL_REQUEST_TEMPLATE.md), whose checklist is
the pre-submission list. Every change ships with its tests, a `CHANGELOG.md` entry under
`## [Unreleased]` if it is user-facing, and its slice's `CONTEXT.md` updated in the same commit. The
maintainer assigns the version.

- **Title**: clear, concise, scoped (e.g. `parser: add struct definition support`)
- **Description**: what problem this solves, what approach was taken, any breaking changes, related issues
- **Tests**: describe what was added and why it provides confidence

A pull request is accepted when CI passes, review approves it, it has no merge conflicts, and VSA
boundaries are preserved.

## Development Tips

| Tool | Use |
|---|---|
| `tools/check_docs_hygiene.py` | The documentation gate: counts, version copies, local-only paths |
| `tools/clean_stale_target.py` | Reclaims disk. Cargo keeps every old build, and a test binary here statically links LLVM. `--dry-run` reports first |
| `tools/regen_gpu_runtime.sh` | Rebuilds the embedded `.ll` runtimes after a change to their `.c` sources |
| `tools/tmlanguage_scopes.mjs <file.nr>` | Prints the scopes the editor grammar really produces |
| `tools/grad_differential.py`, `tools/dlpack_differential.py` | Differential harnesses behind the `neurc` tests: gradients against finite differences, tensors against NumPy |

```bash
# Verbose compiler logging
RUST_LOG=debug cargo run -p neurc -- check examples/basics/hello.nr

# API docs
cargo doc --no-deps --workspace --open
```

## Developer Certificate of Origin (DCO)

Neuro uses the [Developer Certificate of Origin](https://developercertificate.org/) instead of a
separate CLA. Signing off a commit certifies the DCO for that contribution. Where the DCO says "the
open source license indicated in the file", read the [Neuro Shared Source License](LICENSE), which
covers the whole repository.

By signing off you also **explicitly accept the relicensing terms of section 12.3 of the
[LICENSE](LICENSE)**: your contribution may be redistributed under (a) a future version of the Neuro
Shared Source License, (b) the Apache License 2.0, or (c) a license mutually agreed upon in writing
between the Neuro Project and you. Relicensing under any other license, including strong-copyleft
licenses such as the GPL, requires your individual written consent.

### How to sign off

Use `git commit -s` (or `--signoff`); Git appends a trailer using your configured `user.name` and
`user.email`:

```text
Signed-off-by: Your Name <your.email@example.com>
```

If you forgot to sign off, amend with `git commit --amend --signoff`. For a series of commits, use
`git rebase --signoff <base>`.

**Pull requests with unsigned commits will not be merged.**

## License

By submitting a DCO-signed contribution to Neuro, you agree your contribution is licensed under the
[Neuro Shared Source License v2.1](LICENSE) and accept the future-relicensing terms in section 12.3.
The contributor patent grant of section 12.5 applies to all accepted contributions.
