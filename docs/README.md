# NEURO Compiler Documentation

Welcome to the NEURO compiler documentation! This documentation covers all implemented features, architecture decisions, and usage guides.

## Table of Contents

1. [Overview](#overview)
2. [Getting Started](#getting-started)
3. [Architecture](#architecture)
4. [Feature Documentation](#feature-documentation)
5. [Development](#development)

## Overview

NEURO is a compiled programming language designed for AI workloads, featuring:

- **Static typing** with type inference
- **Tensor primitives** for machine learning (Phase 3+)
- **GPU acceleration** support (Phase 5+)
- **Native compilation** via LLVM
- **Vertical Slice Architecture** for maintainability

**Current Status**: Phase 1 - 90% Complete (as of 2025-11-21)

## Getting Started

### Installation

See [README.md](../README.md) for installation instructions.

### Your First NEURO Program

```neuro
// examples/hello.nr
func main() -> i32 {
    val message = "Hello, NEURO!"
    return 0
}
```

### Checking Types

```bash
cargo run -p neurc -- check examples/hello.nr
```

### Compiling (Phase 1 - In Progress)

```bash
cargo run -p neurc -- compile examples/hello.nr
```

## Architecture

NEURO uses **Vertical Slice Architecture (VSA)**, organizing code by business capabilities rather than technical layers.

### Core Principles

1. **Slice Independence**: Each feature is self-contained
2. **Minimal Coupling**: Slices communicate through well-defined interfaces
3. **Shared Infrastructure**: Common utilities (diagnostics, types) in infrastructure layer
4. **Clear Boundaries**: `pub(crate)` by default, `pub` only for entry points

### Project Structure

```
compiler/
├── infrastructure/          # Shared utilities
│   ├── shared-types/       # Common types (Span, Identifier, Literal)
│   ├── source-location/    # Source location tracking
│   ├── diagnostics/        # Error reporting framework
│   └── project-config/     # Configuration management
│
├── lexical-analysis/       # ✅ Tokenization
├── syntax-parsing/         # ✅ AST generation
├── semantic-analysis/      # ✅ Type checking
├── control-flow/           # ⏳ CFG analysis (future)
├── llvm-backend/           # ✅ Code generation
│
└── neurc/                  # Compiler driver
```

### Compilation Pipeline

```
Source Code (.nr)
    ↓
[Lexical Analysis]  → Tokens
    ↓
[Syntax Parsing]    → AST
    ↓
[Semantic Analysis] → Type-checked AST
    ↓
[LLVM Backend]      → Object Code (.o)
    ↓
[Linker]            → Executable
```

## Feature Documentation

### Phase 1 Features (Complete)

| Feature | Status | Documentation |
|---------|--------|---------------|
| **Lexical Analysis** | ✅ Complete | [lexical-analysis.md](features/lexical-analysis.md) |
| **Syntax Parsing** | ✅ Complete | [syntax-parsing.md](features/syntax-parsing.md) |
| **Semantic Analysis** | ✅ Complete | [semantic-analysis.md](features/semantic-analysis.md) |
| **LLVM Backend** | ✅ Complete | [llvm-backend.md](features/llvm-backend.md) |

### Lexical Analysis

Converts source code to tokens with full Unicode support.

**Features**:
- Keywords, operators, delimiters
- Integer literals (decimal, binary, octal, hex)
- Float literals with scientific notation
- String literals with escape sequences
- Line and block comments
- Comprehensive error reporting

[**Read Documentation →**](features/lexical-analysis.md)

### Syntax Parsing

Transforms tokens into an Abstract Syntax Tree (AST).

**Features**:
- Pratt parser for expressions (correct precedence)
- Recursive descent for statements
- Function definitions
- If/else statements
- Variable declarations
- Error recovery

[**Read Documentation →**](features/syntax-parsing.md)

### Semantic Analysis

Type checks and validates the AST.

**Features**:
- Primitive types (i32, i64, f32, f64, bool)
- Function type checking
- Lexical scoping with shadowing
- Multiple error collection (fail-slow)
- Comprehensive error messages with spans

[**Read Documentation →**](features/semantic-analysis.md)

### LLVM Backend

Generates native object code via LLVM.

**Features**:
- LLVM IR generation (inkwell)
- Function codegen with parameters
- Expression codegen (all operators)
- Statement codegen (variables, if/else, return)
- Object code emission
- Opaque pointer support (LLVM 15+)

[**Read Documentation →**](features/llvm-backend.md)

## Language Guide

### Type System (Phase 1)

**Primitive Types**:
```neuro
i32         // 32-bit signed integer
i64         // 64-bit signed integer
f32         // 32-bit float
f64         // 64-bit float
bool        // Boolean (true/false)
```

**Type Inference**:
```neuro
val x = 42        // Inferred as i32
val y = 3.14      // Inferred as f64
val z: i64 = 100  // Explicit type annotation
```

### Functions

```neuro
func add(a: i32, b: i32) -> i32 {
    return a + b
}

func main() -> i32 {
    val result = add(5, 3)
    return result
}
```

### Variables

```neuro
val immutable: i32 = 42      // Immutable (Phase 1)
mut counter: i32 = 0          // Mutable (Phase 2+)
```

### Control Flow

```neuro
func max(a: i32, b: i32) -> i32 {
    if a > b {
        return a
    } else if a < b {
        return b
    } else {
        return 0
    }
}
```

### Operators

**Arithmetic**: `+`, `-`, `*`, `/`, `%`
**Comparison**: `==`, `!=`, `<`, `>`, `<=`, `>=`
**Logical**: `&&`, `||`, `!`

## Development

### Building the Compiler

```bash
# Build entire workspace
cargo build --workspace

# Build specific slice
cargo build -p llvm-backend

# Build with release optimizations
cargo build --workspace --release
```

### Running Tests

```bash
# Run all tests
cargo test --all

# Run tests for specific slice
cargo test -p semantic-analysis

# Run with output
cargo test -- --nocapture
```

### Code Quality

```bash
# Format code
cargo fmt --all

# Run linter
cargo clippy --all-targets --all-features -- -D warnings

# Check formatting
cargo fmt --all -- --check
```

### Adding a New Feature

1. **Create crate**: `cargo new compiler/feature-name --lib`
2. **Add to workspace**: Update root `Cargo.toml`
3. **Design API**: Single public entry point
4. **Implement**: Keep internals `pub(crate)`
5. **Test**: Comprehensive unit and integration tests
6. **Document**: Add to this documentation

See [CLAUDE.md](../CLAUDE.md) for detailed development guidelines.

## Roadmap

See [roadmap.md](../.idea/roadmap.md) for the complete development roadmap.

### Phase 1 (Current - 90% Complete)
- [x] Lexical Analysis
- [x] Syntax Parsing
- [x] Semantic Analysis (Type Checking)
- [x] LLVM Code Generation
- [ ] End-to-end Compilation (neurc driver integration)

### Phase 2 (Next)
- [ ] Structs
- [ ] Arrays
- [ ] Loops (while, for)
- [ ] Enhanced type inference
- [ ] Module system

### Phase 3 (Future)
- [ ] Static tensor types
- [ ] Tensor operations
- [ ] BLAS integration
- [ ] Memory pooling

### Phase 4+ (Advanced)
- [ ] Automatic differentiation
- [ ] GPU support (CUDA)
- [ ] Neural network primitives
- [ ] LSP server

## Contributing

See [CONTRIBUTING.md](../CONTRIBUTING.md) for contribution guidelines.

### Development Principles

1. **98% Confidence**: Only commit changes you're highly confident about
2. **Test First**: Write tests before implementation
3. **VSA Compliance**: Maintain slice independence
4. **Documentation**: Document as you code
5. **Zero Warnings**: All code must pass clippy

## Resources

### Internal Documentation

- [README.md](../README.md) - Project overview
- [CLAUDE.md](../CLAUDE.md) - AI development guidelines
- [VSA_Rust_3_0.xml](../VSA_Rust_3_0.xml) - Architecture specification
- [CONTRIBUTING.md](../CONTRIBUTING.md) - Contribution guide
- [roadmap.md](../.idea/roadmap.md) - Development roadmap
- [syntax.md](../.idea/syntax.md) - Language syntax reference
- [library.md](../.idea/library.md) - Architecture documentation

### External Resources

- [Rust Book](https://doc.rust-lang.org/book/)
- [LLVM Documentation](https://llvm.org/docs/)
- [inkwell](https://thedan64.github.io/inkwell/)
- [Crafting Interpreters](https://craftinginterpreters.com/)

## Examples

### Factorial

```neuro
func factorial(n: i32) -> i32 {
    if n <= 1 {
        return 1
    } else {
        return n * factorial(n - 1)
    }
}

func main() -> i32 {
    return factorial(5)  // Returns 120
}
```

### Fibonacci

```neuro
func fibonacci(n: i32) -> i32 {
    if n <= 1 {
        return n
    } else {
        return fibonacci(n - 1) + fibonacci(n - 2)
    }
}

func main() -> i32 {
    return fibonacci(10)  // Returns 55
}
```

### Arithmetic Expression

```neuro
func calculate() -> i32 {
    val a = 10
    val b = 20
    val c = 5

    val result = (a + b) * c - 15
    return result  // Returns 135
}
```

## FAQ

### What makes NEURO different?

NEURO is designed specifically for AI workloads with:
- Native tensor support (Phase 3+)
- GPU acceleration (Phase 5+)
- Static type checking for safety
- LLVM-based native compilation for performance

### Can I use NEURO now?

Phase 1 is 90% complete. You can:
- ✅ Parse NEURO programs
- ✅ Type check programs
- ✅ Generate LLVM object code
- ⏳ Compile to executable (integration in progress)

### What's the roadmap?

See [roadmap.md](../.idea/roadmap.md) for the complete timeline. Phase 1 targets basic compilation, Phase 2 adds core language features, Phase 3 adds tensor support, Phase 4+ adds neural network features.

### How can I contribute?

See [CONTRIBUTING.md](../CONTRIBUTING.md). We welcome:
- Bug reports
- Feature requests
- Documentation improvements
- Code contributions (following VSA principles)

### What's VSA?

Vertical Slice Architecture organizes code by business features rather than technical layers. Each "slice" is self-contained with minimal dependencies. See [VSA_Rust_3_0.xml](../VSA_Rust_3_0.xml).

## License

GPL-3.0 - See [LICENSE](../LICENSE) for details.

## Acknowledgments

- Built with [Rust](https://www.rust-lang.org/)
- LLVM bindings via [inkwell](https://github.com/TheDan64/inkwell)
- Lexer powered by [logos](https://github.com/maciejhirsz/logos)
- Inspired by modern compiler architecture

---

**Last Updated**: 2025-11-21
**Version**: 0.1.0 (Phase 1 - 90% Complete)
