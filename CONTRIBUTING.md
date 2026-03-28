# Contributing to NEURO

Thank you for your interest in contributing to the NEURO programming language compiler.

## Project Status

NEURO is in **Phase 1.5** — Phase 1 (core MVP) is complete and the project is now stabilizing the backend (LLVM 20 upgrade, string fat pointers) and preparing for Phase 2 (structs, enums, module system). We welcome contributions, but note:

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
export LLVM_SYS_200_PREFIX=/usr/lib/llvm20
# Add to ~/.bashrc / ~/.zshrc to make permanent

# Ubuntu / Debian
wget https://apt.llvm.org/llvm.sh && chmod +x llvm.sh && sudo ./llvm.sh 20
export LLVM_SYS_200_PREFIX=/usr/lib/llvm-20

# macOS
brew install llvm@20
export LLVM_SYS_200_PREFIX=$(brew --prefix llvm@20)
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
2. [docs/README.md](docs/README.md) — technical documentation index
3. [CHANGELOG.md](CHANGELOG.md) — recent changes
4. Each slice's `CONTEXT.md` — purpose, entry point, and dependency contract

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
```

**Scopes**: `lexer`, `parser`, `semantic`, `codegen`, `infra`, `tests`, `docs`, `build`, `ci`

**Examples**:
- `parser: add struct definition parsing`
- `codegen: fix while-loop break target for nested loops`
- `infra: add Span display impl for error formatting`

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

NEURO uses **Vertical Slice Architecture (VSA)**. Each compiler feature is a self-contained crate.

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

# Dump tokens
cargo run -p neurc -- tokens examples/hello.nr

# Check types without codegen
cargo run -p neurc -- check examples/hello.nr
```

## Current Contribution Priorities

### Phase 1.5 (active)

- Refactor string type from C-style null-terminated pointers to fat pointers (`ptr`, `len`)
- Introduce basic ownership/borrow checker groundwork
- MLIR bindings setup (`melior` crate, LLVM/MLIR 20)

### Phase 2 (next)

- Structs (definition, instantiation, field access)
- Methods on structs (`impl` blocks with `self`)
- Enums with associated data
- Pattern matching (exhaustiveness checking)
- `Result<T, E>` and `Option<T>` types
- Multi-file compilation and `import` statements

### Non-Code Contributions

- Documentation improvements and tutorials
- Bug reports with minimal reproducible examples
- Language design feedback on GitHub Discussions

## License

By contributing to NEURO, you agree your contributions will be licensed under [GPL v3.0](LICENSE).
