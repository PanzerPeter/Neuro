# Contributing to Neuro

Thank you for your interest in contributing to the Neuro programming language compiler.

## Project Status

Neuro is in **Phase 1.5 / Phase 2** — Phase 1 (core MVP) is complete, the LLVM 20 backend and string fat pointers have landed, and Phase 2 is underway (structs ✅, methods ✅). Remaining Phase 2 work covers enums, pattern matching, and the module system. We welcome contributions, but note:

- Architecture and design are still evolving
- Breaking changes are expected between minor versions
- Current focus is Phase 1.5 stabilization and Phase 2 language features

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

#### 1. Parser & Syntax Fixes

- [x] **Fix `else if` condition — `no_struct_lit` guard missing.** Bare identifier used as `else if` condition consumed the block `{` as a struct literal opener, corrupting the parse tree. (`parser.rs:544`)
- [x] **`const` declarations** (syntax.md §1.3). Compile-time constants at module and function scope.
- [x] **Compound assignment operators**: `+=`, `-=`, `*=`, `/=`, `%=` (§1.4). Desugar to `target = target OP expr` at parse time.
- [x] **`as` type cast** (§1.4). Explicit numeric type conversion: `val x: f64 = n as f64`. LLVM `sext` / `zext` / `trunc` / `fpext` / `fptrunc` / `fptosi` / `sitofp` emission.
- [x] **Inclusive range `..=` in `for` loops** (§1.6). `for i in 0..=10` was explicitly rejected at parse time.
- [x] **Bitwise operators**: `&`, `|`, `^`, `~`, `<<` (§1.4). Right shift is exposed as `.shr(n)` method per spec, not an operator. Precedence levels `Shl > BitAnd > BitXor > BitOr` land between `Comparison` and `Sum`.
- [x] **Integer literal type suffixes**: `42i64`, `255u8` (§1.4).
- [x] **`if` and block expressions as values** (§1.8).
- [x] **Float literal suffixes**: `1.5f32`, `2.0f64` (§1.4). Mirrors integer-suffix lexer/parser/type-inference plumbing for floats.
- [x] **Comparison chain rejection** (§1.4). `a < b < c` is a compile error with a "use `&&` to combine separate comparisons" suggestion. Detected in semantic analysis when a comparison operator's LHS is itself a comparison expression.
- [ ] **Underscore digit separators in numeric literals** (§1.2). `1_000_000`, `0xFF_FF`, `0b1010_0011`. Update the lexer's `logos`-based regex patterns for integer and float tokens to allow `_` between digit groups; strip underscores before passing the raw string to Rust's `parse::<T>()`. No AST or codegen changes required.

#### 2. Language Semantics

- [x] **IEEE-754 native float comparison** (§1.2, §3.10). `<`, `>`, `<=`, `>=` wired directly to LLVM `fcmp olt/ogt/ole/oge`. Does not dispatch through the `Comparable` trait.
- [x] **Integer literal magnitude rule** (§1.3). Remove silent `i32 → i64` promotion for out-of-range literals.
- [x] **`while true` lint** (§3.7). Emits `warning[prefer-loop-over-while-true]`; suppressed with `@allow(prefer_loop_over_while_true)`. The general attribute system landed as a side-effect.
- [x] **`??` operator — R-to-L associativity confirmed + parser test** (§3.11). Lexer/AST/parser only. Semantic rejects with `OperatorNotYetSupported`; full unwrap semantics deferred to Phase 2 with `Option`/`Result`.
- [ ] **`*Assign` traits — scalar path** (§3.10). Declare `AddAssign`, `SubAssign`, `MulAssign`, `DivAssign`, `RemAssign` traits in the stdlib with `&mut self` receivers. Wire compound-assignment lowering in `semantic-analysis/` and `llvm-backend/`: (1) if the LHS type implements the matching `*Assign` trait, emit a direct `lhs.op_assign(rhs)` call; (2) otherwise fall back to `lhs = lhs OP rhs`. Primitive scalars (`Copy`) always take path #2 — no behavioral change for existing code. The tensor in-place path (path #1 via DLPack) is deferred to Phase 3.
- [ ] **Integer overflow semantics** (§1.2). Debug builds: integer arithmetic must panic on overflow (use LLVM's `llvm.sadd.with.overflow` / `llvm.uadd.with.overflow` intrinsics and emit a conditional `abort`). Release builds: wrap silently (default two's complement, no intrinsic change). Also add `wrapping_add`, `wrapping_sub`, `wrapping_mul`, `saturating_add`, `saturating_sub`, `checked_add`, `checked_sub`, `checked_mul` methods on every integer primitive; these lower to the matching LLVM intrinsics. Touch `llvm-backend/` for codegen, `semantic-analysis/` for method resolution, and the stdlib crate for trait declarations.

#### 3. String Memory Model (groundwork only — full ownership is Phase 1.7)

- [x] **Refactor string type — fat pointers** (`ptr`, `len`).
- [x] **String equality operators** (`==` and `!=`).
- [ ] **String literal vs runtime string distinction** (§2.7). Literals are stored in `.rodata` and are never heap-allocated. Add a note in the `llvm-backend/` codegen that the fat-ptr `len` field counts UTF-8 bytes and excludes any null terminator; add a compile-time assertion that downstream consumers must not rely on null termination. No ABI change — documentation and an assertion comment in the relevant codegen path.
- [ ] **`&string` slice type** (§2.7). Teach the type checker in `semantic-analysis/` to accept `&string` as a distinct type from `string`: a borrowed, non-owning fat-pointer view `(ptr, len)` into UTF-8 data. Codegen is a no-op (the ABI is already a fat pointer). This is the prerequisite for `.slice(range)` and `.char_slice(range)` in later phases.

---

### Phase 1.7 — Ownership & Borrow Checker (next up)

**Goal:** Deterministic, zero-overhead memory management. No GC, no ARC.
Touches `semantic-analysis/` heavily; `llvm-backend/` gains a `Drop` lowering pass.

- [ ] **Move semantics by default.** Assignment and function-call argument passing move ownership for non-`Copy` types. The source binding becomes invalid after the move. Add move-tracking to the type checker; emit "use of moved value" errors.
- [ ] **`Copy` trait + `@derive(Copy, Clone)`.** Built-in for all primitive scalars. Structs may derive `Copy` only when all fields are `Copy`. Validation in `semantic-analysis/`.
- [ ] **`.clone()` method.** Explicit deep copy for non-`Copy` owned types; removes any implicit deep copies elsewhere in the compiler.
- [ ] **Immutable borrows `&T`.** Any number may coexist. Borrow checker rejects mutable borrows during an active immutable borrow. Implement the borrow checker as a new pass in `semantic-analysis/`.
- [ ] **Mutable borrows `&mut T`.** At most one `&mut T` at a time; excludes immutable borrows. Dereference through `*` for read/write.
- [ ] **Lifetime inference + explicit annotations.** Elision rules: single input lifetime → all outputs; `&self` lifetime → method outputs. Explicit `<'a>` for advanced patterns.
- [ ] **`Drop` trait + deterministic destruction.** Destructor runs when owner goes out of scope. `llvm-backend/` must emit Drop calls at scope-exit basic blocks.
- [ ] **Remove ARC plumbing.** Strip any reference-counting code introduced during the alpha; replace entirely with owned-or-borrowed semantics.
- [ ] **Runtime string ops behind the borrow checker.** `String::new`, `string + &string` concat, `.push_str`, `.clear`. First features to exercise heap + Drop.
- [ ] **`unsafe { }` block infrastructure.** Parse `unsafe` as a reserved keyword, add an `UnsafeBlock` AST node. Outside `@kernel` bodies the block is inert (no general unsafe semantics yet). Needed as groundwork for Phase 5 GPU kernels.

---

### Phase 1.8 — HIR & MLIR Backend Plumbing (upcoming)

**Goal:** Build the typed High-Level IR and `melior` infrastructure that every backend from Phase 3 onward depends on. Split from Phase 2 because stabilizing the HIR contract before adding tensor types prevents costly rewrites later.

- [ ] **Integrate `melior`** (Rust MLIR bindings) alongside `inkwell`. Build against LLVM/MLIR 20; verify both bindings share the same dylib.
- [ ] **`neuro-hir` infrastructure crate.** New crate at `compiler/infrastructure/neuro-hir/`. Defines a typed HIR — the stable contract between the frontend (parser + type checker) and all backends. Both `llvm-backend` and the future `mlir-backend` lower from HIR, not from the AST directly.
- [ ] **HIR lowering strategy.** Implement the lowering pipeline: `AST → neuro-hir → llvm-backend (inkwell → native)`. The `mlir-backend` slot is scaffolded but empty until Phase 3.
- [ ] **Migrate `llvm-backend` off AST onto HIR.** Acceptance criterion: full test suite passes with HIR-routed codegen before any tensor code is added.
- [ ] **`mlir-backend` slice scaffold.** Empty slice that consumes HIR and produces a trivial MLIR module. Wires the `melior` dependency, the lowering entry point, and a CI smoke test.

---

### Phase 2 — Core Language (after Phase 1.7 & 1.8)

**Goal:** Complete the general-purpose surface language with safe memory semantics.

#### 2A. Type System Expansion

- [ ] **Arrays `[T; N]`** (§3.1). Fixed-size arrays: indexing, `.len()`, iteration via `for x in arr` and `for (i, x) in arr.enumerate()`. Bounds check: debug panic, release wrap; investigate compile-time elision.
- [ ] **Tuples and destructuring** (§3.2). `(T1, T2, ...)`, `.0`/`.1` field access, `val (a, b) = pair`, struct and array destructuring, `_` wildcard, nested patterns.
- [ ] **Enums with associated data** (§3.5). Tagged-union codegen. `enum Foo { Bar, Baz(i32), Qux { x: f64 } }`.
- [ ] **Pattern matching** (§3.6). `match` expression with exhaustiveness checking; guard clauses. Required prerequisite for `Option`/`Result` ergonomics.
- [ ] **Type aliases** (§3.14). `type Vec3 = Tensor<f32, [3]>` — transparent, non-distinct alias.
- [ ] **Newtype declarations** (§3.15). `newtype Meters(f64)` — distinct nominal type, zero overhead. Forwards `Copy`/`Clone` from inner type; all other traits implemented explicitly.
- [ ] **Struct shorthand + update syntax** (§3.3). `Point { x, y }` shorthand; `Point { x: 1.0, ..p }` spread from existing value.

#### 2B. Generics, Traits, and Abstractions

- [ ] **Generics** (§3.8). `func max<T: Ord>(a: T, b: T) -> T`. Monomorphization-based — no runtime dispatch. Add generic parameters to functions, structs, and `impl` blocks; implement type unification in `semantic-analysis/`.
- [ ] **Trait declarations** (§3.9). `trait Drawable { fn draw(&self); }`. Trait bounds, default methods, associated types.
- [ ] **Operator traits — scalar path** (§3.10). `Add`, `Sub`, `Mul`, `Div`, `Rem`, `Neg`, `Not`, `BitAnd`, `BitOr`, `BitXor`, `Shl`, `Shr`, `Eq`, `Ord`. Includes `*Assign` traits from Phase 1.5.
- [ ] **Closures and lambdas** (§3.12). `|x| x + 1`, `|x: i32| -> i32 { x + 1 }`. Three capture modes (`Fn` / `FnMut` / `FnOnce`) determined by usage. Borrow-checker integration required.

#### 2C. Error Handling, Modules, and Prelude

- [ ] **`Option<T>` and `Result<T, E>` in stdlib** (§3.11). Add as built-in generic enums; wire into the type checker.
- [ ] **`??` operator — full implementation** (§3.11). Accepts `Option<T>` or `Result<T, E>` on the LHS; `Result` Err payload is discarded. Single `Coalesce<T>` trait. Fallback expression is lazy. R-to-L associativity already pinned by Phase 1.5 parser test.
- [ ] **`val-else else |binding|` type-directed binding** (§8.2). `Result<T, E>` → `|name|` binds `E`. `Option<T>` → `|_|` only.
- [ ] **Error propagation operator `?`**. Sugar over `match … { Ok(v) => v, Err(e) => return Err(e.into()) }`.
- [ ] **Multi-file compilation** (§3.16). Each `.nr` file = one module. Directories with `mod.nr` form hierarchies. Implement in `neurc/` driver and `semantic-analysis/`.
- [ ] **`import` statements and visibility** (§3.16). `import math`, `import math::{sqrt, sin}`, `import math::matrix as mat`, relative paths, variant imports (`import Option::{Some, None}`).
- [ ] **`export` keyword** (§3.3, §3.16). Items and struct fields private by default; `export` opts into module-public visibility.
- [ ] **Inline `module { }` blocks + `export import` re-export** (§3.16).
- [ ] **Implicit prelude** (§3.16). Auto-import `std::prelude::{Option, Some, None, Result, Ok, Err, println, print}`. `@no_prelude` opts out.

#### 2D. Language Cleanup

- [ ] **String interpolation `"Hello, {name}!"`** (§1.7). Stateful lexer rewrite. Format mini-language: `{x:.2}`, `{n:08d}`, `{s:^10}`, etc. Escape `\{` for a literal brace.
- [ ] **Triple-quoted strings with dedent** (§1.7). `"""..."""` block; closing delimiter column determines dedent amount. Content lines with less indentation than the closing delimiter are a compile error.
- [ ] **Nested block comments** (§1.1). `/* outer /* inner */ still outer */`. Requires a hand-written comment scanner — `logos` longest-match cannot handle nesting.
- [ ] **Named arguments** (§3.13). `external internal: T` parameter form; callers may pass positionally or by name. Lowers to identical IR — no runtime cost.

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
