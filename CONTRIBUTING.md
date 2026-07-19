# Contributing to Neuro

Thank you for your interest in contributing to the Neuro programming language compiler.

## Project Status

Neuro is in **Phase 1 — Core Language** (v1.x), the umbrella phase covering the full general-purpose language. It is divided into lettered sub-phases, implemented strictly in dependency order. **Sub-phase 1F (Generics, Traits & Dispatch) is the active sub-phase.** Completed so far: 1A (core MVP), 1B (syntax & semantics stabilization — casts, bitwise ops, literal suffixes, if/block expressions, builtin-method dispatch, integer overflow, etc.), 1C (ownership & borrow checker — move semantics, `Copy`, `&T`/`&mut T`, borrow exclusivity, lifetime elision, `&mut self`, deterministic `Drop`; one flagged item remains), 1D (HIR / MLIR plumbing — typed HIR, AST→HIR lowering, HIR-routed LLVM backend, `mlir-backend` scaffold), and 1E (type system — structs, methods, arrays, tuples, destructuring, type aliases, enums, pattern matching, and newtypes). Completing all of Phase 1 (1A–1H) ships **v2.0.0** and opens Phase 2 (Tensors). We welcome contributions, but note:

- Architecture and design are still evolving
- Breaking changes are expected between minor versions

## Table of Contents

1. [Getting Started](#getting-started)
2. [Development Workflow](#development-workflow)
3. [Coding Standards](#coding-standards)
4. [Architecture Guidelines](#architecture-guidelines)
5. [Testing Requirements](#testing-requirements)
6. [Submitting Changes](#submitting-changes)

## Getting Started

### Prerequisites

- **Rust**: 1.85 or later (`rustup update stable`)
- **LLVM 20**: development package (see below)
- **Git**
- **IDE**: VS Code with rust-analyzer recommended

### Setting Up the Development Environment

```bash
# Arch Linux / CachyOS
sudo pacman -S llvm20
export LLVM_SYS_201_PREFIX=/usr/lib/llvm20
# Add to ~/.bashrc / ~/.zshrc to make permanent

# Ubuntu / Debian
wget https://apt.llvm.org/llvm.sh && chmod +x llvm.sh && sudo ./llvm.sh 20
export LLVM_SYS_201_PREFIX=/usr/lib/llvm-20

# macOS
brew install llvm@20
export LLVM_SYS_201_PREFIX=$(brew --prefix llvm@20)
```

> **Optional — MLIR backend (sub-phase 1D+):** the `mlir-backend` slice is gated
> behind the off-by-default `mlir` cargo feature, so a normal build needs only
> LLVM 20. To work on it you need an LLVM 20 install that includes MLIR plus a
> matching libclang 20 (`MLIR_SYS_200_PREFIX` / `TABLEGEN_200_PREFIX` /
> `LIBCLANG_PATH`); see [Optional: MLIR Backend](docs/getting-started/installation.md#optional-mlir-backend-phase-18).

```bash
# Clone and verify the build
git clone https://github.com/PanzerPeter/Neuro.git
cd Neuro
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

### Understanding the Codebase

Before contributing, read:

1. [README.md](README.md) — project overview and installation
2. [DESIGN.md](DESIGN.md) — language design principles and non-goals
3. [docs/README.md](docs/README.md) — technical documentation index
4. [CHANGELOG.md](CHANGELOG.md) — recent changes
5. Each slice's `CONTEXT.md` — purpose, entry point, and dependency contract

## Development Workflow

### Before You Start

1. Check open issues and discuss major changes before starting work
2. Verify your contribution aligns with the current roadmap phase
3. For architectural changes, open an issue first

### Working on a Feature

1. `git checkout -b feature/your-feature-name`
2. Implement following coding and architecture standards
3. Write tests (unit + integration) for all new functionality
4. Pass all quality gates (see below)
5. Submit a pull request with a clear description

### Commit Message Format

```
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

Every commit **must** carry a `Signed-off-by:` trailer. See
[Developer Certificate of Origin (DCO)](#developer-certificate-of-origin-dco)
below for the legal effect and the `git commit -s` workflow.

## Coding Standards

### Rust Rules

- **No `unwrap()`/`expect()` in production paths** — use `Result<T, E>` with actionable errors
- **Explicit integer widths** on public-facing APIs (`u32`, `i64`, not `usize` for domain values)
- **`pub(crate)` by default** — only slice entry points are `pub`
- **Borrowing over cloning** — avoid unnecessary `.clone()`
- **Document public APIs** with `///` doc comments; document every `unsafe` block with a safety rationale

### Quality Gates

All PRs must pass:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Architecture Guidelines

Neuro uses **Vertical Slice Architecture (VSA)**. Each compiler feature is a self-contained crate.

### Project Layout

```
compiler/
├── infrastructure/          # Shared utilities — no business logic
│   ├── ast-types/           # AST node definitions (owned here, not in syntax-parsing)
│   ├── neuro-hir/           # Typed HIR — backend-agnostic frontend↔backend contract (1D)
│   ├── shared-types/        # Span, Identifier, Literal
│   ├── diagnostics/         # Error type infrastructure
│   ├── source-location/     # Source mapping
│   └── project-config/      # neuro.toml config parsing
│
├── lexical-analysis/        # Tokenizer slice
├── syntax-parsing/          # Parser slice (depends on lexical-analysis by design)
├── semantic-analysis/       # Type checker slice
├── hir-lowering/            # AST → typed HIR lowering slice (1D)
├── control-flow/            # CFG analysis slice (not yet active)
├── llvm-backend/            # LLVM 20 / inkwell 0.9 codegen slice
├── mlir-backend/            # MLIR / melior slice (1D+, off-by-default `mlir` feature)
│
└── neurc/                   # Compiler driver — the only crate that depends on all slices
```

### VSA Rules

**Do:**
- Organize by features, not layers
- Keep slices independent — duplication is preferred over coupling
- Use `pub(crate)` as default visibility; `pub` only for the slice entry point
- Accept infrastructure dependencies (`shared-types`, `diagnostics`, `ast-types`)
- Keep each slice's `CONTEXT.md` up to date when entry points or dependencies change

**Do not:**
- Import one feature slice from another (except the intentional `syntax-parsing` → `lexical-analysis` pipeline coupling)
- Put business logic in infrastructure crates
- Use `unwrap()` in production code paths
- Expose slice internals via permissive `pub use`

### Adding a New Feature Slice

1. `cargo new compiler/feature-name --lib`
2. Add to workspace `Cargo.toml` members
3. Depend only on infrastructure crates
4. Design a single public entry point
5. Keep all internals `pub(crate)`
6. Write comprehensive tests
7. Add a `CONTEXT.md` with Purpose, Entry Point, Data Ownership, Shared Kernel, and Notes sections

## Testing Requirements

All contributions must include:

- **Unit tests** for individual functions (`#[cfg(test)] mod tests { ... }` in source file)
- **Integration tests** in the crate's `tests/` directory for slice entry points
- **Edge cases** — boundary conditions and error paths
- **Regression tests** for bug fixes

### Test Organization

```bash
# Run all tests
cargo test --workspace

# Run tests for one slice
cargo test -p lexical-analysis

# Run a specific test by name
cargo test test_tokenize_string_escapes

# Show test output
cargo test -- --nocapture
```

### Test Quality

- Tests must be isolated — no shared mutable state between tests
- Tests must be deterministic — same input always produces the same result
- Test names must be descriptive and self-documenting
- Avoid testing implementation details; test observable behavior

## Submitting Changes

### Pre-Submission Checklist

- [ ] All tests pass: `cargo test --workspace`
- [ ] Clippy clean: `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] Formatted: `cargo fmt --all`
- [ ] `CHANGELOG.md` updated for user-facing changes
- [ ] `CONTEXT.md` updated if entry points or dependencies changed
- [ ] Commit messages follow the format above
- [ ] Every commit carries a `Signed-off-by:` trailer (DCO, see below)
- [ ] Branch is up to date with `main`

### Pull Request Guidelines

- **Title**: clear, concise, scoped (e.g. `parser: add struct definition support`)
- **Description**: what problem this solves, what approach was taken, any breaking changes, related issues
- **Tests**: describe what was added and why it provides confidence

### Acceptance Criteria

- All CI checks pass
- Code review approved
- No merge conflicts
- VSA boundaries preserved
- Coding standards met
- Adequate tests included

## Development Tips

```bash
# Watch and auto-rebuild on changes
cargo install cargo-watch
cargo watch -x "build --workspace"

# Faster test runner
cargo install cargo-nextest
cargo nextest run --workspace

# Generate API docs
cargo doc --no-deps --workspace --open

# Security audit
cargo audit
```

### Debugging

```bash
# Verbose logging
RUST_LOG=debug cargo run -p neurc -- check examples/basics/hello.nr

# Check types without codegen
cargo run -p neurc -- check examples/basics/hello.nr

# Compile to native binary
cargo run -p neurc -- compile examples/basics/hello.nr
```

## Current Contribution Priorities

### Phase 1 — Core Language: current priorities

The active sub-phase is **1F (Generics, Traits & Dispatch)** — sub-phase 1E is
complete. The roadmap is dependency-ordered, so pick the **topmost open item** — its
prerequisites are already done. Coordinate on an issue before starting a large item.

**Recently completed — 1E (Type System):**

- [x] **Enums with associated data** (§3.5). Tagged-union codegen:
      `enum Foo { Bar, Baz(i32), Qux { x: f64 } }`. Prerequisite for pattern matching.
      Landed v1.53.0 (unit/tuple/struct variants, scalar payloads; `match` is next).
- [x] **Pattern matching** (§3.6). `match` with exhaustiveness checking + guards;
      required for `Option`/`Result` ergonomics in 1G. Landed v1.54.0 (enum
      deconstruction, value/or/range/wildcard patterns, guards; scrutinee is
      enum/integer/`char`/`bool`).
- [x] **Newtype declarations** (§3.15). `newtype Meters = f64` — distinct nominal
      type, zero overhead; construction `Meters(3.5)`, inner access `.0`. Landed
      v1.55.0 (Copy-inner only; backends erase the wrapper). Completes 1E.

**Open now — 1F (Generics, Traits & Dispatch):** generic *functions* (§3.8) landed
in v1.56.0, generic *structs & impls* in v1.57.0, *const parameters, `where`
clauses & turbofish* in v1.58.0 (all monomorphized, type arguments inferred from
value/field arguments or written explicitly, bounds parsed-but-unenforced, `Copy`
arguments only; const parameters inferred from array lengths / field values or
supplied by turbofish, `where` value predicates checked per instantiation), and
*explicit lifetime annotations* `<'a>` in v1.59.0 (a well-formedness surface —
declared in the generic list, validated, then erased; `&'a T` == `&T`), and
*trait declarations* (§3.9) in v1.60.0 (required + default methods,
`impl Trait for Type` conformance checking, and enforced trait bounds on
generics — all fully monomorphized and erased, so trait bounds that were merely
parsed before are now checked), *operator traits* (§3.10) in v1.61.0 (the built-in
`Add`/`Sub`/`Neg`/`PartialEq`/`Comparable` family on `Copy` structs, desugared to
method calls), and *static & dynamic dispatch* (§3.17) in v1.62.0 (`impl Trait` in
argument and return position, monomorphized; `&dyn Trait` / `&mut dyn Trait` trait
objects dispatched through a per-(trait, type) vtable, with object safety enforced).
The topmost open item is now closures and lambdas (§3.12).

**Next, in dependency order:** 1F (closures) → 1G (error
handling, collections, modules, prelude) → 1H (string interpolation, triple-quoted
strings, nested comments, named arguments). See the [Quick Roadmap](README.md#quick-roadmap).

`[x]` = landed · `[ ]` = open. See [CHANGELOG.md](CHANGELOG.md) and the README
capabilities table for full behavior.

**Recently landed (sub-phase 1C — Ownership & Borrow Checker):**

- [x] Move semantics by default (§2.2, v1.29.0) — `.clone()` opts out.
- [x] `.clone()` builtin on `string` (§2.7, v1.27.0).
- [x] `Copy` trait + `@derive(Copy, Clone)` on structs (§2.3, v1.30.0).
- [x] Immutable borrows `&T` (§2.4, v1.31.0).
- [x] Mutable borrows `&mut T` + `*` deref operator (§2.5, v1.33.0).
- [x] Flow-sensitive borrow exclusivity — shared XOR mutable (§2.4/§2.5, v1.39.0).
- [x] Lifetime elision + returned-reference outlives check (§2.6, v1.40.0).
- [x] `&mut self` methods — in-place receiver mutation (§2.5, v1.41.0).
- [x] Panic runtime — `panic`/`assert`/`unreachable`, abort, no unwinding (§1.2).
- [x] `unsafe { }` block infrastructure — inert outside `@kernel` (§3).
- [x] `&string` slice type — borrowed `(ptr, len)` UTF-8 view; byte-level `==`/`!=` (§2.7, v1.32.0).
- [x] Remove ARC — audit: no reference-counting plumbing ever existed (v1.41.6).
- [x] String concatenation `+` — `malloc`+`memcpy` → new owned immutable `string` (§2.7, v1.42.0).

- [x] **`Drop` trait + deterministic destruction** (v1.44.0). Runs at scope exit (normal exit only, never on panic); a compiler-known lang-item like `Copy`/`Clone` (no general trait system needed), dropping a moved-out value exactly once. Deferred: reassignment does not drop the prior value; struct `Drop` fields are not auto-dropped.
- [x] **String `.slice(range)`** (v1.43.0). Borrowed `&string` sub-slice (zero copy); panics on an out-of-bounds range or a mid-codepoint boundary in both builds.

⚑ **One flagged 1C item remains:** growable runtime string ops (`String::new` /
`.push_str` / `.clear`) are blocked by the immutable-`string` spec contradiction.
Recommendation pending sign-off: relocate to **1G** alongside the heap-backed
collections. It does not block 1E onward.

Explicit lifetime annotations `<'a>` landed in **1F** (v1.59.0): a `'a` lifetime
token, a `lifetimes` list on function/struct/impl definitions kept separate from
monomorphizable generics, and `&'a T` / `&'a mut T` reference annotations. They are
validated against the in-scope lifetime parameters (`UndeclaredLifetime`) then erased
— `&'a T` is the same type as `&T`. Lifetime *elision* (v1.40.0) still covers the
common cases and does the real outlives checking.

### Non-Code Contributions

- Documentation improvements and tutorials
- Bug reports with minimal reproducible examples
- Language design feedback on GitHub Discussions

## Developer Certificate of Origin (DCO)

Neuro uses the **Developer Certificate of Origin** ([developercertificate.org](https://developercertificate.org/)) — the same lightweight, no-paperwork mechanism used by the Linux kernel, Git, Docker, and many other open projects — instead of a separate CLA.

By signing off on a commit, you certify that:

> 1. The contribution was created in whole or in part by you and you have the right to submit it under the open source license indicated in the file; or
> 2. The contribution is based upon previous work that, to the best of your knowledge, is covered under an appropriate open source license and you have the right under that license to submit that work with modifications; or
> 3. The contribution was provided directly to you by some other person who certified (1), (2), or (3) and you have not modified it.
> 4. You understand and agree that this project and the contribution are public and that a record of the contribution (including all personal information you submit with it, including the sign-off) is maintained indefinitely and may be redistributed consistent with this project and the open source license(s) involved.

In addition, by signing off you **explicitly accept the relicensing terms of § 12.3 of the [LICENSE](LICENSE)**: your contribution may be redistributed under (a) a future version of the Neuro Shared Source License, (b) the Apache License 2.0, or (c) a license mutually agreed upon in writing between the Neuro Project and you. Relicensing under any other license — including strong-copyleft licenses such as the GPL — requires your individual written consent.

### How to sign off

Use `git commit -s` (or `--signoff`); Git will append a trailer using your configured `user.name` and `user.email`:

```bash
git commit -s -m "parser: add struct definition parsing"
```

Resulting trailer:

```
Signed-off-by: Your Name <your.email@example.com>
```

If you forgot to sign off, amend with `git commit --amend --signoff`. For a series of commits, use `git rebase --signoff <base>`.

**Pull requests with unsigned commits will not be merged.** The DCO check is enforced in CI.

## License

By submitting a DCO-signed contribution to Neuro, you agree your contribution is licensed under the [Neuro Shared Source License v2.1](LICENSE) and accept the future-relicensing terms enumerated in § 12.3. The contributor patent grant of § 12.5 applies to all accepted contributions.
