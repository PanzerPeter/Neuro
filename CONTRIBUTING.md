# Contributing to the NEURO Programming Language

Welcome to the NEURO Programming Language project! We're building a high-performance, AI-first programming language using Vertical Slice Architecture (VSA) principles. This guide will help you understand our development approach and contribute effectively.

## Table of Contents

- [Quick Start](#quick-start)
- [VSA Architecture Overview](#vsa-architecture-overview)
- [Development Workflow](#development-workflow)
- [Feature Implementation Process](#feature-implementation-process)
- [Testing Strategy](#testing-strategy)
- [Quality Standards](#quality-standards)
- [VSA Slice Structure](#vsa-slice-structure)
- [Decision Framework](#decision-framework)
- [Getting Help](#getting-help)

## Quick Start

1. **Fork and Clone**
   ```bash
   git clone https://github.com/PanzerPeter/Neuro.git
   cd Neuro
   ```

2. **Verify Setup**
   ```bash
   cargo check --workspace
   cargo test --workspace --lib
   ```

3. **Pick a Task**
   - Check `idea/todo.txt` for current priorities
   - Look for `good first issue` labels on GitHub
   - Start with Phase 1 tasks from `idea/roadmap.txt`

## VSA Architecture Overview

NEURO uses **Vertical Slice Architecture** to organize code by business capabilities rather than technical layers. Each slice represents a complete business feature that can be developed, tested, and deployed independently.

### Core VSA Principles

1. **Feature-Based Organization**: Group components by business capability
2. **Slice Independence**: High cohesion within slices, loose coupling between slices
3. **Duplication Over Coupling**: Prefer duplicated code over shared abstractions that create dependencies
4. **Business Value Focus**: Each slice represents measurable business value

### Current VSA Slices

**Compiler Features:**
- `lexical-analysis`: Tokenization and lexical processing
- `syntax-parsing`: AST generation and syntax analysis
- `type-system`: Type inference, checking, and constraint solving
- `control-flow`: If/else, loops, pattern matching
- `memory-management`: ARC, memory pools, leak detection
- `automatic-differentiation`: Gradient computation and #[grad] attribute
- `gpu-compilation`: CUDA/Vulkan kernel generation
- `neural-networks`: DSL for model definition
- `tensor-operations`: Tensor types and operations
- `error-handling`: Diagnostics and error recovery
- `optimization`: Code optimization passes
- `code-generation`: LLVM IR generation
- `symbol-resolution`: Name resolution and scoping

**Runtime Features:**
- `tensor-runtime`: Tensor operations and memory management
- `gpu-runtime`: GPU execution and memory management

**Infrastructure:**
- `diagnostics`: Error reporting and source location tracking
- `source-location`: Span tracking and source maps
- `shared-types`: Common types used across slices

**Tooling:**
- `compiler-driver`: The `neurc` command-line interface
- `package-manager`: `neurpm` for package management

## Development Workflow

### Branch Strategy
```bash
# Feature branches
git checkout -b feature/parser-functions
git checkout -b feature/type-inference

# Bug fixes  
git checkout -b fix/lexer-unicode-bug
```

### Commit Messages (Conventional Commits)
```bash
feat(parser): implement function declaration parsing
test(type-system): add property tests for type inference
docs(spec): update parser specification
fix(lexer): resolve unicode tokenization issue
refactor(gpu): simplify kernel generation logic
```

## Feature Implementation Process

Follow this **Test-Driven Development (TDD)** workflow for every feature:

### 1. Analysis & Planning
- [ ] Read feature specification in `spec/` documents
- [ ] Identify which VSA slice(s) the feature belongs to
- [ ] Check dependencies on other features/slices
- [ ] Review existing similar implementations for patterns
- [ ] Create feature branch: `git checkout -b feature/feature-name`

### 2. Design Phase
- [ ] Define the feature's public API and types
- [ ] Plan integration points with existing slices
- [ ] Design error handling and diagnostics
- [ ] Create placeholder VSA slice if new (`Cargo.toml` + `lib.rs`)
- [ ] Add feature to workspace `Cargo.toml` members if needed

### 3. Test-First Implementation
- [ ] Write failing unit tests for the feature
- [ ] Write failing integration tests if applicable
- [ ] Ensure tests fail for the right reasons (not compilation errors)
- [ ] Run: `cargo test -p <slice-name> --lib` (should fail)

### 4. Core Implementation
- [ ] Implement minimal code to make tests pass
- [ ] Follow VSA principles - keep slice dependencies minimal
- [ ] Add comprehensive error handling and diagnostics
- [ ] Use existing infrastructure (diagnostics, source-location, shared-types)
- [ ] Run: `cargo test -p <slice-name> --lib` (should pass)

### 5. Integration Verification
- [ ] Run all workspace tests: `cargo test --workspace --lib`
- [ ] Fix any broken tests in other slices
- [ ] Ensure no compilation errors: `cargo check --workspace`
- [ ] Run specific integration tests if they exist

### 6. Example Creation
- [ ] Create example file in `examples/` directory
- [ ] Name: `<feature-name>.nrl` or add to existing relevant example
- [ ] Include comprehensive comments explaining the feature
- [ ] Test compilation: `neurc examples/<example>.nrl --check`
- [ ] Add example to `examples/README.md` with description

### 7. Documentation Update
- [ ] Update relevant `spec/` documents
- [ ] Update main `README.md` if it's a major feature
- [ ] Add to `CHANGELOG.md` with feature description
- [ ] Add inline documentation comments to public APIs
- [ ] Update `idea/roadmap.txt` to mark feature as completed

### 8. Final Verification
- [ ] Full workspace build: `cargo build --release`
- [ ] Full test suite: `cargo test --workspace`
- [ ] Lint check: `cargo clippy --workspace`
- [ ] Format check: `cargo fmt --check`
- [ ] Example compilation test for all examples
- [ ] Git commit with descriptive message

## Testing Strategy

### Test Organization
```
slice-name/
├── src/
├── tests/           # Integration tests for the slice
│   ├── basic_functionality.rs
│   ├── error_conditions.rs
│   └── integration_with_other_slices.rs
└── Cargo.toml

tests/               # Workspace-wide integration tests
├── integration/
│   ├── end_to_end_compilation.rs
│   ├── example_programs.rs
│   └── cross_slice_interactions.rs
└── property_tests/  # Property-based tests using proptest
```

### Testing Priorities
1. **Unit Tests**: Test slice handlers (business logic) in isolation
2. **Integration Tests**: Test slice boundaries and interactions
3. **Contract Tests**: Test slice interfaces
4. **Property Tests**: Use `proptest` for complex invariants

### Test Coverage Goals
- **Unit Tests**: 80%+ coverage in slice handlers
- **Integration Tests**: All slice boundaries tested
- **Examples**: All examples must compile and run

## Quality Standards

### Code Quality Gates
Before merging any feature:
- [ ] All tests pass: `cargo test --workspace`
- [ ] No compilation warnings in release mode
- [ ] Clippy passes: `cargo clippy --workspace -- -D warnings`
- [ ] Code formatted: `cargo fmt --check`
- [ ] Examples compile successfully
- [ ] Documentation updated
- [ ] Performance regression check if applicable

### Performance Considerations
- Profile compilation time impact
- Monitor memory usage in compiler phases
- Benchmark runtime performance for generated code
- Consider incremental compilation impact
- Optimize hot paths identified by profiling

Use `criterion` for benchmarking:
```bash
cd benchmarks
cargo bench
```

## VSA Slice Structure

### Standard Slice Layout
```
compiler/feature-name/
├── Cargo.toml           # Slice dependencies
├── src/
│   ├── lib.rs          # Public API
│   ├── types.rs        # Domain types
│   ├── handler.rs      # Core business logic
│   ├── error.rs        # Error types
│   └── integration.rs  # Integration with other slices
├── tests/
│   ├── unit_tests.rs
│   ├── integration_tests.rs
│   └── error_handling_tests.rs
└── README.md           # Slice documentation
```

### Slice Dependencies
- **Infrastructure Only**: Slices should only depend on infrastructure components
- **No Cross-Slice Dependencies**: Avoid direct dependencies between feature slices
- **Communication**: Use events or shared types for inter-slice communication

### Example Cargo.toml for a Slice
```toml
[package]
name = "syntax-parsing"
version = "0.1.0"
edition = "2021"

[dependencies]
# Infrastructure dependencies only
diagnostics = { path = "../infrastructure/diagnostics" }
source-location = { path = "../infrastructure/source-location" }
shared-types = { path = "../infrastructure/shared-types" }

# External dependencies
serde = { version = "1.0", features = ["derive"] }
tracing = "0.1"

[dev-dependencies]
proptest = "1.0"
tokio-test = "0.4"
```

## Decision Framework

### When to Create a New Slice
Use this decision tree:

1. **Does this represent a complete business use case?**
   - Yes → Create separate slice
   - No → Continue to question 2

2. **Is this a variation with different business rules?**
   - Yes → Extend existing slice with configuration
   - No → Include in broader business capability

### When to Share Code Between Slices
1. **Is this pure infrastructure with no business logic?**
   - Yes → Safe to share in infrastructure layer
   - No → Continue to question 2

2. **Same logic in 3+ slices causing maintenance pain?**
   - No → Keep duplicated (premature optimization)
   - Yes → Continue to question 3

3. **Would sharing require coordination between slices?**
   - Yes → Keep duplicated (slice independence priority)
   - No → Consider extraction with versioned interface

### VSA Applicability Check
Before applying VSA patterns:
- [ ] Are there 3+ distinct business features?
- [ ] Do features have independent business value?
- [ ] Is this more complex than simple CRUD operations?
- [ ] Would traditional layering actually work fine here?

## Getting Help

### Resources
- **Issues**: Check existing GitHub issues for similar problems
- **Discussions**: Use GitHub Discussions for questions
- **Documentation**: Check `spec/` directory for language specifications
- **Examples**: Review `examples/` directory for usage patterns

### Communication
- **Bug Reports**: Use GitHub Issues with clear reproduction steps
- **Feature Requests**: Open GitHub Issues for discussion first
- **Questions**: Use GitHub Discussions for general questions
- **Real-time Chat**: [Add Discord/Slack link if available]

### Development Support
- **VSA Questions**: Reference `idea/VSA_2.5.xml` for architectural guidance
- **Implementation Help**: Check `idea/todo.txt` for current priorities
- **Roadmap**: See `idea/roadmap.txt` for project direction

## Code of Conduct

This project follows the [Contributor Covenant Code of Conduct](CODE_OF_CONDUCT.md). By participating, you agree to uphold this code.

---

**Remember**: When in doubt, favor simplicity over complexity, duplication over coupling, and slice independence over shared abstractions. The goal is to build a maintainable, AI-first programming language that can evolve rapidly while maintaining quality.

Thank you for contributing to NEURO! 🚀