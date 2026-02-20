# Contributing to NEURO

Thank you for your interest in contributing to the NEURO programming language! This document provides guidelines and information for contributors.

## Project Status

NEURO is currently in **Phase 1 (alpha development)** with the core MVP completed. The project is focused on stabilization and incremental improvements. We welcome contributions, but please note:

- The project is in early alpha stage
- Architecture and design are still evolving
- Breaking changes are frequent
- Current focus is on completing Phase 1 and stabilizing the core compiler

## Table of Contents

1. [Getting Started](#getting-started)
2. [Development Workflow](#development-workflow)
3. [Coding Standards](#coding-standards)
4. [Architecture Guidelines](#architecture-guidelines)
5. [Testing Requirements](#testing-requirements)
6. [Submitting Changes](#submitting-changes)
7. [Community Guidelines](#community-guidelines)

## Getting Started

### Prerequisites

- **Rust**: 1.70 or later
- **LLVM**: 18.1.8 (for backend development)
- **Git**: For version control
- **IDE**: VS Code with rust-analyzer recommended

### Setting Up Development Environment

```bash
# Clone the repository
git clone https://github.com/PanzerPeter/Neuro.git
cd Neuro

# Build the workspace
cargo build --workspace

# Run tests for individual slices
cargo test -p lexical-analysis
cargo test -p syntax-parsing
cargo test -p semantic-analysis

# Check code quality
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

### Understanding the Codebase

Before contributing, please read:

1. [README.md](README.md) - Project overview
2. [docs/README.md](docs/README.md) - Technical documentation
3. [CHANGELOG.md](CHANGELOG.md) - Recent changes and release notes

## Development Workflow

### Before You Start

1. **Check existing issues**: Look for open issues or create a new one to discuss your idea
2. **Review current priorities**: Ensure your contribution aligns with the current phase and open issues
3. **Ask questions**: Use GitHub Discussions or Issues if you're unsure about anything

### Working on a Feature

1. **Create a branch**: `git checkout -b feature/your-feature-name`
2. **Make changes**: Follow coding standards and architecture guidelines
3. **Write tests**: All new functionality must have comprehensive tests
4. **Run checks**: Ensure all quality checks pass before committing
5. **Commit**: Use descriptive commit messages (see format below)
6. **Push and PR**: Create a pull request with a clear description

### Commit Message Format

Use the following format for commit messages:

```
scope: short summary (50 chars or less)

Longer description if needed. Explain what and why, not how.
Reference issues with #issue-number.
```

**Examples**:
- `lexer: add support for multiline comments`
- `parser: fix operator precedence for tensor operations`
- `infra: improve error span tracking accuracy`
- `tests: add comprehensive tests for type inference`

**Scopes**: `lexer`, `parser`, `semantic`, `codegen`, `infra`, `tests`, `docs`, `build`, `ci`

## Coding Standards

### Rust Idioms

- **Error Handling**: Prefer `Result<T, E>` over panics in production code
- **No unwrap/expect**: Avoid in production; use proper error handling
- **Explicit Types**: Use explicit integer widths (`u32`, `i64`) for domain constraints
- **Zero-Cost Abstractions**: Prefer iterators and inline functions
- **Borrowing over Cloning**: Avoid unnecessary clones

### Code Quality Requirements

All contributions MUST:

1. **Pass clippy**: `cargo clippy --all-targets --all-features -- -D warnings`
2. **Be formatted**: `cargo fmt --all`
3. **Have tests**: Comprehensive unit tests for all new functionality
4. **Pass all tests**: All existing tests must continue to pass
5. **Be documented**: Public APIs must have doc comments (`///`)

### Module Visibility

```rust
// Default to pub(crate) for internal slice items
pub(crate) struct InternalHelper { }

// Use pub only for slice entry points
pub fn tokenize(input: &str) -> TokenStream { }

// For submodules within a slice
pub(super) fn helper() { }

// BAD: Don't expose internals
// pub use other_slice::Type;
```

### Documentation Standards

- **Public APIs**: Must have `///` doc comments with examples
- **Unsafe code**: Must document safety requirements
- **Complex algorithms**: Inline comments explaining the approach
- **No emojis**: Do not use emojis in code or log messages (project policy)

## Architecture Guidelines

NEURO uses **Vertical Slice Architecture (VSA)**. Understanding and following VSA principles is essential for contributions.

### VSA Core Principles

1. **Slice Independence**: Each feature is self-contained
2. **Minimal Coupling**: Slices communicate through well-defined interfaces
3. **Shared Infrastructure**: Common utilities only (no business logic)
4. **Clear Boundaries**: `pub(crate)` by default, `pub` only for entry points

### Project Structure

```
compiler/
├── infrastructure/          # Shared utilities (no business logic)
│   ├── shared-types/       # Common types
│   ├── diagnostics/        # Error reporting
│   ├── source-location/    # Source tracking
│   └── project-config/     # Configuration
│
├── lexical-analysis/       # Feature slice: Tokenization
├── syntax-parsing/         # Feature slice: AST generation
├── semantic-analysis/      # Feature slice: Type checking
├── llvm-backend/           # Feature slice: Code generation
│
└── neurc/                  # Compiler driver (orchestrates slices)
```

### Adding a New Feature Slice

1. **Create crate**: `cargo new compiler/feature-name --lib`
2. **Add to workspace**: Update root `Cargo.toml` members
3. **Add dependencies**: Only infrastructure crates (shared-types, diagnostics)
4. **Design API**: Single public entry point
5. **Implement**: Keep internals `pub(crate)`
6. **Test**: Comprehensive tests in the slice
7. **Document**: Add markdown doc in `docs/features/`

### VSA Rules (Critical)

DO:
- Organize by features, not layers
- Keep slices independent
- Accept code duplication to avoid coupling
- Use strict types (newtypes) for domain concepts
- Use `pub(crate)` as default visibility

DON'T:
- Share business logic between slices
- Create "God" traits or types
- Import feature slices from other features
- Use `unwrap()` in production code
- Expose internals via permissive `pub`

## Testing Requirements

### Test Coverage Standards

All contributions must include:

1. **Unit tests**: For individual functions and methods
2. **Integration tests**: For slice entry points
3. **Edge cases**: Boundary conditions and error cases
4. **Regression tests**: For bug fixes

### Running Tests

```bash
# Run all tests for a slice
cargo test -p lexical-analysis

# Run specific test
cargo test test_name

# Run with output
cargo test -- --nocapture

# Run all tests (note: may fail on Windows with LLVM linking issues)
cargo test --all
```

### Test Organization

- Tests in same file as code: `#[cfg(test)] mod tests { ... }`
- Integration tests: `tests/` directory in crate
- Test names: Descriptive, prefix with `test_` or use `#[test]`

### Test Quality

- **Isolated**: Tests should not depend on each other
- **Deterministic**: Same input always produces same output
- **Fast**: Tests should complete quickly
- **Clear**: Test names and assertions should be self-documenting

## Submitting Changes

### Pre-Submission Checklist

Before creating a pull request, ensure:

- [ ] All tests pass: `cargo test -p <your-crate>`
- [ ] Clippy passes: `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] Code is formatted: `cargo fmt --all`
- [ ] Documentation is updated (if adding features)
- [ ] CHANGELOG.md is updated (for user-facing changes)
- [ ] Commit messages follow the format
- [ ] Branch is up to date with main

### Pull Request Guidelines

1. **Title**: Clear, concise description of changes
2. **Description**: Explain what, why, and how
   - What problem does this solve?
   - What approach did you take?
   - Any breaking changes?
   - Related issues?
3. **Tests**: Describe what tests were added
4. **Documentation**: Note any doc updates
5. **Screenshots**: If UI-related (future)

### Code Review Process

1. Automated checks run (CI/CD)
2. Maintainer reviews code
3. Feedback is provided
4. You address feedback
5. Approved and merged

### Acceptance Criteria

For your PR to be merged:

- All CI checks pass
- Code review approved by maintainer
- No merge conflicts
- Follows VSA architecture
- Meets coding standards
- Has adequate tests and documentation

## Community Guidelines

### Code of Conduct

We expect all contributors to:

- Be respectful and professional
- Welcome newcomers
- Accept constructive criticism
- Focus on what's best for the project
- Show empathy towards other community members

### Communication Channels

- **GitHub Issues**: Bug reports, feature requests
- **GitHub Discussions**: Questions, ideas, general discussion
- **Pull Requests**: Code contributions

### Getting Help

If you're stuck:

1. Check existing documentation
2. Search closed issues for similar problems
3. Ask in GitHub Discussions
4. Create an issue with a clear description

## Areas for Contribution

### Current Priorities (Phase 1)

- End-to-end compilation to executable
- Integration tests for full compiler pipeline
- Documentation improvements
- Bug fixes in existing features

### Future Areas (Phase 2+)

Planned feature areas include:
- Structs and enums
- Loops (while, for)
- Module system
- Enhanced type inference
- Arrays and strings

### Non-Code Contributions

We also welcome:

- **Documentation**: Tutorials, guides, examples
- **Bug reports**: Detailed, reproducible issues
- **Testing**: Try the compiler, report issues
- **Design feedback**: Architecture and API suggestions

## Development Tips

### Common Commands

```bash
# Build specific slice
cargo build -p lexical-analysis

# Watch and auto-rebuild
cargo install cargo-watch
cargo watch -x "build --workspace"

# Faster test runner
cargo install cargo-nextest
cargo nextest run --all

# Security audit
cargo audit

# Generate documentation
cargo doc --no-deps --workspace
```

### Debugging

- Use `RUST_LOG=debug cargo run` for verbose output
- Use `cargo run -p neurc -- check <file>` to test parsing
- Check LLVM IR: Will be available in future versions

### Performance Profiling

- Use `criterion` for benchmarks (when added)
- Profile before optimizing
- Focus on algorithmic improvements first

## Questions?

If you have questions about contributing:

1. Check [docs/README.md](docs/README.md) for technical documentation
2. Search existing issues and discussions
3. Create a new issue or discussion

## License

By contributing to NEURO, you agree that your contributions will be licensed under the [GNU General Public License v3.0](LICENSE).

## Acknowledgments

Thank you for contributing to NEURO! Every contribution, no matter how small, helps make NEURO better.

---

**Note**: This is an alpha-stage project. Contribution guidelines may evolve as the project matures.

**Last Updated**: 2026-02-20
**Phase**: 1 (Alpha Development - Core MVP Complete)
