# NEURO Documentation

Welcome to the NEURO compiler documentation. This guide covers everything from getting started to advanced compiler internals.

**Current Status**: Phase 1 (Core MVP Complete) - Alpha Development

## Quick Links

- [Installation Guide](getting-started/installation.md) - Set up NEURO on your system
- [Quick Start](getting-started/quick-start.md) - Get up and running in 5 minutes
- [Language Reference](language-reference/types.md) - Complete language documentation
- [Troubleshooting](guides/troubleshooting.md) - Common issues and solutions

## Documentation Structure

### Getting Started

Recommended for new NEURO users:

- **[Installation Guide](getting-started/installation.md)** - Install NEURO on Windows, Linux, or macOS
- **[Quick Start Guide](getting-started/quick-start.md)** - Basic usage and workflow
- **[Your First Program](getting-started/first-program.md)** - Step-by-step tutorial

### Language Reference

Complete reference for the NEURO language:

- **[Types](language-reference/types.md)** - Type system (primitives, integers, floats, booleans)
- **[Variables](language-reference/variables.md)** - Variables, mutability, and reassignment
- **[Functions](language-reference/functions.md)** - Function declarations, parameters, returns
- **[Expressions](language-reference/expressions.md)** - Expression syntax and evaluation
- **[Control Flow](language-reference/control-flow.md)** - If/else statements and conditionals
- **[Operators](language-reference/operators.md)** - Arithmetic, comparison, logical operators

### User Guides

Practical guides for everyday use:

- **[CLI Usage](guides/cli-usage.md)** - Complete command-line reference
- **[Troubleshooting](guides/troubleshooting.md)** - Common problems and solutions

### Compiler Architecture

Deep dive into compiler internals:

- **[Contribution Guidelines](../CONTRIBUTING.md)** - Architecture rules and contribution standards
- **[Compilation Pipeline](compiler/compilation.md)** - End-to-end compilation process
- **[Components](compiler/components/)** - Individual compiler components:
  - [Lexical Analysis](compiler/components/lexical-analysis.md) - Tokenization
  - [Syntax Parsing](compiler/components/syntax-parsing.md) - AST generation
  - [Semantic Analysis](compiler/components/semantic-analysis.md) - Type checking
  - [LLVM Backend](compiler/components/llvm-backend.md) - Code generation

## What is NEURO?

NEURO is a compiled programming language with native code generation via LLVM. It combines:

- **Static typing** with inference for safety and performance
- **Tensor primitives** for machine learning (Phase 3+)
- **GPU acceleration** support (Phase 5+)
- **Native compilation** via LLVM for maximum speed
- **Modern syntax** inspired by Rust, Python, and Swift

## Current Status (Phase 1, Core MVP Complete)

The compiler currently supports:

### Implemented Features

- **Types**:
  - Primitive types: i8, i16, i32, i64, u8, u16, u32, u64, f32, f64, bool
  - Contextual type inference for numeric literals (with range validation)
  - String type with escape sequences (\n, \t, \", \\, \xNN, \u{NNNN})
  - Function types
  - Void type

- **Variables**:
  - Immutable variables (`val`)
  - Mutable variables (`mut`)
  - Variable reassignment
  - Lexical scoping with shadowing

- **Functions**:
  - Function declarations with parameters
  - Explicit return statements
  - Expression-based returns (implicit returns)
  - Recursion
  - Forward references

- **Control Flow**:
  - If/else statements
  - Else-if chaining
  - Block scoping

- **Operators**:
  - Arithmetic: `+`, `-`, `*`, `/`, `%`
  - Comparison: `==`, `!=`, `<`, `>`, `<=`, `>=`
  - Logical: `&&`, `||`, `!`
  - Unary: `-`, `!`

- **Compilation**:
  - Full LLVM backend
  - Native executable generation
  - Comprehensive error messages
  - 312 tests passing across all components

### Planned Phases

- **Phase 2**: Structs, arrays, loops, generics, module system
- **Phase 3**: Tensor types and operations
- **Phase 4**: Automatic differentiation
- **Phase 5**: GPU acceleration (CUDA)
- **Phase 6**: Neural network library

Phase goals are summarized here and refined through project discussions.

## Example Programs

### Hello World

```neuro
func main() -> i32 {
    return 0
}
```

### Factorial

```neuro
func factorial(n: i32) -> i32 {
    if n <= 1 {
        1  // Implicit return
    } else {
        n * factorial(n - 1)
    }
}

func main() -> i32 {
    factorial(5)  // Returns 120
}
```

### Mutable Variables

```neuro
func counter() -> i32 {
    mut count: i32 = 0
    count = count + 1
    count = count + 1
    count = count + 1
    count  // Returns 3
}
```

More examples in [examples/](../examples/) directory.

## Development

### Building from Source

```bash
git clone https://github.com/PanzerPeter/Neuro.git
cd Neuro
cargo build --release
cargo test --all
```

### Running the Compiler

```bash
# Check syntax and types
cargo run -p neurc -- check examples/hello.nr

# Compile to executable
cargo run -p neurc -- compile examples/milestone.nr

# Run compiled program
./examples/milestone  # Unix
.\examples\milestone.exe  # Windows
```

### Contributing

See [CONTRIBUTING.md](../CONTRIBUTING.md) for:
- Code style guidelines
- VSA architecture principles
- Testing requirements
- Pull request process

## Getting Help

### Documentation

- **Getting Started**: Start with [Installation](getting-started/installation.md)
- **Language Questions**: Check [Language Reference](language-reference/types.md)
- **Compiler Issues**: See [Troubleshooting](guides/troubleshooting.md)
- **CLI Help**: Read [CLI Usage](guides/cli-usage.md)

### Support

- **Issues**: [GitHub Issues](https://github.com/PanzerPeter/Neuro/issues)
- **Discussions**: [GitHub Discussions](https://github.com/PanzerPeter/Neuro/discussions)
- **Development**: See [CONTRIBUTING.md](../CONTRIBUTING.md)

## Project Resources

### Core Documentation

- [README.md](../README.md) - Project overview
- [CHANGELOG.md](../CHANGELOG.md) - Version history
- [CONTRIBUTING.md](../CONTRIBUTING.md) - Contribution guidelines
- [LICENSE](../LICENSE) - GPL v3.0

### Development Resources

- [CONTRIBUTING.md](../CONTRIBUTING.md) - Development workflow and architecture guidelines
- [CHANGELOG.md](../CHANGELOG.md) - Change history and milestone tracking

### Tools

- [VSCode Extension](../neuro-language-support/) - Syntax highlighting

## Key Features by Phase

### Phase 1 (Current, Core MVP Complete)

Core compiler functionality:
- All primitive types (integers, floats, bool, string)
- Functions with expression-based returns
- Mutable variables and reassignment
- Control flow (if/else)
- Comprehensive type checking
- Native code generation via LLVM

### Phase 2 (Planned)

Core language features:
- Structs and custom types
- Arrays and tuples
- Loops (while, for)
- Generics
- Module system
- Error handling (Result, Option)

### Phase 3 (Planned)

Tensor foundation:
- Static tensor types
- Tensor operations
- BLAS integration
- Shape checking

### Phase 4 (Planned)

Automatic differentiation:
- Reverse-mode AD
- Gradient computation
- Training loop utilities

### Phase 5 (Planned)

GPU acceleration:
- CUDA support
- GPU kernels
- Device memory management

### Phase 6 (Planned)

Neural network library:
- Common layers (Dense, Conv2d, etc.)
- Optimizers (SGD, Adam)
- Loss functions
- Model API

## Architecture Principles

NEURO uses **Vertical Slice Architecture (VSA)**:

1. **Slice Independence**: Each feature is self-contained
2. **Infrastructure Sharing**: Common utilities in infrastructure layer
3. **Clear Boundaries**: Minimal public API, `pub(crate)` by default
4. **No Cross-Slice Imports**: Features don't import from each other

Benefits:
- Independent development
- Clear ownership
- Better testability
- Parallel compilation

## Compilation Pipeline

```
Source File (.nr)
    ↓
[Lexical Analysis]  → Tokenization
    ↓
[Syntax Parsing]    → AST Generation
    ↓
[Semantic Analysis] → Type Checking
    ↓
[LLVM Backend]      → Object Code (.o)
    ↓
[System Linker]     → Native Executable
```

Each stage is independent and can be tested separately. See [Compilation Pipeline](compiler/compilation.md) for details.

## Testing

The compiler includes comprehensive test coverage.

- **Total**: 312 tests passing across the workspace

Run tests:
```bash
cargo test --all
```

## Performance Goals

### Phase 1 (Current)

- Compilation time: <1s for small programs
- Type checking: <100ms
- No runtime overhead from language features

### Future Phases

- Arithmetic operations: 2-5x slower than C++ (vs 100x for Python)
- Neural network training: Within 10% of PyTorch performance
- Memory usage: 1-2x of equivalent C++ programs

## Language Design Principles

1. **Safety**: Static typing prevents errors at compile time
2. **Performance**: Native compilation for maximum speed
3. **Simplicity**: Clean, readable syntax
4. **Explicitness**: Prefer explicit over implicit
5. **Zero-cost abstractions**: High-level features with no runtime cost

## Community and Ecosystem

NEURO is an open-source project under active development:

- **License**: GPL v3.0
- **Language**: Rust 1.70+
- **Target**: LLVM 18.1.8
- **Stage**: Alpha (Phase 1)
- **Stability**: Breaking changes expected

## Acknowledgments

NEURO draws inspiration from:

- **Rust**: Ownership semantics, type system, syntax
- **Python**: Simplicity, AI ecosystem integration
- **Swift**: Modern language design principles
- **Mojo**: AI-first design philosophy

Built with:
- [Rust](https://www.rust-lang.org/) - Systems programming language
- [LLVM](https://llvm.org/) - Compiler infrastructure
- [inkwell](https://github.com/TheDan64/inkwell) - Safe LLVM bindings
- [logos](https://github.com/maciejhirsz/logos) - Lexer generator

## Next Steps

### For Users

1. [Install NEURO](getting-started/installation.md)
2. Follow the [Quick Start Guide](getting-started/quick-start.md)
3. Write [Your First Program](getting-started/first-program.md)
4. Explore the [Language Reference](language-reference/types.md)

### For Contributors

1. Read [CONTRIBUTING.md](../CONTRIBUTING.md)
2. Review architecture and visibility rules in the contributing guide
3. Check [CHANGELOG.md](../CHANGELOG.md) for recent direction

### For Researchers

1. Review [Architecture Documentation](compiler/components/)
2. Study [Compilation Pipeline](compiler/compilation.md)
3. Examine [Type System Design](language-reference/types.md)

## Stay Updated

- Watch the [GitHub repository](https://github.com/PanzerPeter/Neuro)
- Check [CHANGELOG.md](../CHANGELOG.md) for updates

---

**Last Updated**: 2026-02-20
**Version**: Phase 1 (Core MVP Complete)
**Status**: Alpha Development - Ready for Phase 2
