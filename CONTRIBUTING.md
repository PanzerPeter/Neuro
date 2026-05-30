# Contributing to Neuro

Thank you for your interest in contributing to the Neuro programming language compiler.

## Project Status

Neuro is in **Phase 1.5 — Syntax & Semantics Stabilization** (the active phase). Phase 1 (core MVP) is complete; the LLVM 20 backend, string fat pointers, structs, and methods have all landed. Phase 1.7 (ownership & borrow checker) and Phase 1.8 (HIR / MLIR plumbing) follow before the bulk of Phase 2. We welcome contributions, but note:

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
│   ├── shared-types/        # Span, Identifier, Literal
│   ├── diagnostics/         # Error type infrastructure
│   ├── source-location/     # Source mapping
│   └── project-config/      # neuro.toml config parsing
│
├── lexical-analysis/        # Tokenizer slice
├── syntax-parsing/          # Parser slice (depends on lexical-analysis by design)
├── semantic-analysis/       # Type checker slice
├── control-flow/            # CFG analysis slice (Phase 2+)
├── llvm-backend/            # LLVM 20 / inkwell 0.8 codegen slice
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
RUST_LOG=debug cargo run -p neurc -- check examples/hello.nr

# Check types without codegen
cargo run -p neurc -- check examples/hello.nr

# Compile to native binary
cargo run -p neurc -- compile examples/hello.nr
```

## Current Contribution Priorities

### Phase 1.5 — Syntax & Semantics Stabilization (active)

**Goal:** Stabilize syntax, ABI, and semantic rules before adding ownership.
Everything in this phase is a frontend / type-checker / scalar-codegen change.
The borrow checker and HIR/MLIR plumbing live in Phase 1.7 and 1.8.

`[x]` = landed · `[ ]` = open / good first-issue candidate.
Open items are ordered by dependency — earlier ones unblock later ones.

#### 1. Parser & Syntax Fixes — complete

- [x] **Fix `else if` condition — `no_struct_lit` guard missing.** Bare identifier used as `else if` condition consumed the block `{` as a struct literal opener, corrupting the parse tree.
- [x] **`const` declarations** (§1.3). Compile-time constants at module and function scope.
- [x] **Compound assignment operators**: `+=`, `-=`, `*=`, `/=`, `%=` (§1.4). Desugar to `target = target OP expr` at parse time.
- [x] **`as` type cast** (§1.4). Explicit numeric conversion with LLVM `sext` / `zext` / `trunc` / `fpext` / `fptrunc` / `fptosi` / `sitofp` emission.
- [x] **Inclusive range `..=` in `for` loops** (§1.6).
- [x] **Bitwise operators**: `&`, `|`, `^`, `~`, `<<` (§1.4). Precedence `Shl > BitAnd > BitXor > BitOr`. (Right shift is the `.shr(n)` method, not an operator — it is *not* yet implemented; tracked under §2 once builtin-method dispatch exists.)
- [x] **Integer literal type suffixes**: `42i64`, `255u8` (§1.4).
- [x] **`if` and block expressions as values** (§1.8).
- [x] **Float literal suffixes**: `1.5f32`, `2.0f64` (§1.4).
- [x] **Comparison chain rejection** (§1.4). `a < b < c` is a compile error suggesting `&&`.
- [x] **Underscore digit separators** (§1.2). `1_000_000`, `0xFF_FF`, `0b1010_0011`.

#### 2. Language Semantics

- [x] **IEEE-754 native float comparison** (§1.2, §3.10). `<`, `>`, `<=`, `>=` wired directly to LLVM `fcmp`; no `Comparable` dispatch.
- [x] **Integer literal magnitude rule** (§1.3). No silent `i32 → i64` promotion for out-of-range literals.
- [x] **`while true` lint** (§3.7). `warning[prefer-loop-over-while-true]`, suppressed with `@allow(prefer_loop_over_while_true)`. Brought in the general attribute system.
- [x] **`??` operator — parsed, R-to-L associativity + parser test** (§3.11). Semantic rejects with `OperatorNotYetSupported`; full unwrap lands in Phase 2C with `Option`/`Result`.
- [ ] **Builtin method dispatch on primitive & string types.** Methods today resolve only on structs (via `impl` blocks). Teach `semantic-analysis/` + `llvm-backend/` to dispatch a fixed, compiler-known set of intrinsic methods on builtin types. **Prerequisite** for the integer methods and `.shr(n)` below (and later for string `.len()` / `.slice()`).
- [x] **Integer overflow semantics** (§1.2). Debug builds panic on overflow (LLVM `*.with.overflow` intrinsics + conditional `abort`); release builds wrap (two's complement). Codegen-only — no method-dispatch prerequisite.
- [ ] **Integer primitive methods — `wrapping_*` / `saturating_*` + `.shr(n)`** (§1.2, §1.4). `wrapping_add/sub/mul`, `saturating_add/sub/mul`, and the spec's right-shift `.shr(n)`. Depends on builtin method dispatch. `checked_*` returns `Option<T>` → deferred to Phase 2C.

> **Moved out of Phase 1.5 (forward dependency):** the `*Assign` traits
> (`AddAssign`/`SubAssign`/…) need the trait system and now live in Phase 2B.
> For `Copy` scalars, compound assignment already desugars to `x = x OP rhs`,
> so nothing regresses in the meantime.

#### 3. String Memory Model (groundwork only — full ownership is Phase 1.7)

- [x] **Refactor string type — fat pointers** (`ptr`, `len`).
- [x] **String equality operators** (`==` and `!=`).
- [ ] **String literal vs runtime string distinction** (§2.7). Literals live in `.rodata` and are never heap-allocated. Formalize, document, and test that the fat-ptr `len` counts UTF-8 bytes and excludes the null terminator. Codegen already computes `len` this way — no ABI change.

> **Moved out of Phase 1.5 (forward dependency):** the `&string` slice type is a
> borrowed `(ptr, len)` view and depends on the reference type `&T`, which lands
> in Phase 1.7 — it is now tracked there.

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
