# NEURO Programming Language

**Status:** Alpha Development - Not Production Ready

[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)

A modern, compiled programming language designed for high-performance AI development. NEURO combines the productivity of Python with the speed of C++, featuring static typing with inference, zero-cost abstractions, and native GPU acceleration.

## What is NEURO?

NEURO is an Ahead-of-Time (AOT) compiled language built from the ground up for AI workloads. Unlike Python, which serves as an interpreted glue language for C++ libraries, NEURO compiles directly to native code via LLVM, eliminating runtime overhead while maintaining clean, expressive syntax.

### Key Features

- **First-Class Tensor Types**: Tensors are language primitives with compile-time shape verification
- **Native Automatic Differentiation**: Built-in `@grad` attribute for zero-overhead backpropagation
- **GPU Integration**: Native CUDA/Vulkan kernel generation with `@gpu` and `@kernel` attributes
- **Performance**: Near-C++ performance with 2-5x overhead vs 100x for interpreted languages
- **Modern Type System**: Type inference with static guarantees and generics
- **Memory Management**: Automatic Reference Counting (ARC) with optional memory pool blocks

## Quick Example

```rust
import neuro.nn as nn
import neuro.opt as opt

@model
struct MnistNet {
    conv1: nn.Conv2d<in=1, out=32, k=3>,
    pool: nn.MaxPool2d<2>,
    fc: nn.Dense<128>,
    out: nn.Dense<10>
}

func main() -> Result<(), Error> {
    val model = MnistNet::new()
    mut optimizer = opt.Adam(model.params, lr=1e-3)

    val dataset = load_mnist()?
        .to_device(gpu(0))

    pool {
        for epoch in 0..10 {
            for batch in dataset.batch(64) {
                @grad(model) {
                    val logits = model.forward(batch.x)
                    val loss = nn.cross_entropy(logits, batch.y)
                }
                optimizer.step()
            }
            print(f"Epoch {epoch} complete")
        }
    }

    model.save("mnist.nrm")?
    Ok(())
}
```

## Installation

**Note:** NEURO is in active development. Installation instructions will be provided as the compiler matures.

### Prerequisites

- Rust 1.70 or later
- LLVM 16 or later
- CMake 3.20+
- (Optional) CUDA Toolkit 12.0+ for GPU support

### Building from Source

```bash
# Clone the repository
git clone https://github.com/yourusername/neuro-lang.git
cd neuro-lang

# Build the compiler (Note: requires LLVM 16+)
cargo build --release

# Run tests (individual crates)
cargo test -p lexical-analysis
cargo test -p syntax-parsing
cargo test -p semantic-analysis

# Test the compiler
cargo run -p neurc -- check examples/hello.nr

# Install the compiler (optional)
cargo install --path compiler/neurc
```

**Known Issue (Windows)**: Full workspace tests (`cargo test --all`) may fail with LLVM linker errors on Windows if `libxml2` is not installed. The compiler itself builds and works correctly. Individual crate tests work fine. This is a test harness linking issue, not a runtime issue.

## Project Status

NEURO is currently in **Phase 1** of development (Roadmap v3.0) - **90% Complete**. We have successfully built the core compiler pipeline with lexical analysis, parsing, type checking, and LLVM code generation.

### Current Capabilities (Phase 1 - 90% Complete)

- [x] **Lexical Analysis** - Complete tokenizer with Unicode support (28 tests)
- [x] **Syntax Parsing** - Complete Pratt parser for expressions and recursive descent for statements (65 tests)
- [x] **Semantic Analysis** - Full type checking with lexical scoping and error collection (24 tests)
- [x] **LLVM Backend** - Complete code generation to native object code (4 tests)
- [x] **CLI Compiler** - `neurc check` command for syntax and type validation
- [ ] End-to-end compilation to executable (in progress - 10% remaining)

### Roadmap Overview

- **Phase 1** (3-4 months): Minimal viable compiler
- **Phase 2** (3-4 months): Core language features (structs, modules, generics)
- **Phase 3** (3-4 months): Tensor foundation and operations
- **Phase 4** (4-6 months): Automatic differentiation
- **Phase 5** (4-6 months): GPU acceleration
- **Phase 6** (3-4 months): Neural network library

## Architecture

NEURO follows **Vertical Slice Architecture (VSA)** principles, organizing code by business capabilities rather than technical layers. This enables:

- Independent feature development
- Clear ownership boundaries
- Better testability
- Parallel compilation

### Project Structure

```
neuro-lang/
├── compiler/           # Core compiler (VSA feature slices)
│   ├── lexical-analysis/
│   ├── syntax-parsing/
│   ├── semantic-analysis/
│   ├── tensor-operations/
│   ├── llvm-backend/
│   └── infrastructure/
├── runtime/            # Runtime libraries
│   ├── neuro-std/
│   ├── neuro-nn/
│   └── neuro-gpu/
├── tools/              # Development tools
│   ├── neuro-lsp/
│   └── neuro-fmt/
└── tests/              # Integration tests
```

## Language Syntax

NEURO uses clean, familiar syntax inspired by Rust, Python, and TypeScript:

### Variables and Types

```rust
// Immutable by default
val x: i32 = 42
val name: string = "NEURO"

// Explicit mutability
mut counter: i32 = 0
counter = counter + 1

// Type inference
val pi = 3.14159  // inferred as f64
```

### Functions

```rust
func add(a: i32, b: i32) -> i32 {
    return a + b
}

// Expression-based return
func multiply(a: i32, b: i32) -> i32 {
    a * b  // implicit return
}
```

### Tensor Types

```rust
// Static tensor with compile-time shape checking
val matrix: Tensor<f32, [3, 3]> = [
    [1.0, 2.0, 3.0],
    [4.0, 5.0, 6.0],
    [7.0, 8.0, 9.0]
]

// Matrix multiplication
val result = matrix @ matrix

// Element-wise operations
val doubled = matrix * 2.0
```

## Development

### Building the Compiler

```bash
# Build all workspace members
cargo build --workspace

# Build in release mode
cargo build --workspace --release

# Run specific crate
cargo run -p neurc -- --help
```

### Running Tests

```bash
# Run all tests
cargo test --all

# Run tests for specific slice
cargo test -p lexical-analysis

# Run with output
cargo test -- --nocapture
```

### Code Quality

```bash
# Run clippy linter
cargo clippy --all-targets --all-features

# Format code
cargo fmt --all

# Check formatting
cargo fmt --all -- --check
```

## Contributing

NEURO is an open-source project and welcomes contributions. However, please note:

1. The project is in early alpha stage
2. Architecture and design are still evolving
3. Breaking changes are frequent
4. Focus is on core compiler functionality (Phase 1)

Before contributing, please:
- Read the [VSA Guidelines](VSA_Rust_3_0.xml)
- Check existing issues and roadmap
- Discuss major changes in issues first

### Development Guidelines

- Follow Rust idioms and best practices
- Use `pub(crate)` for internal slice items
- Ensure slice independence (minimal cross-slice dependencies)
- Write tests for all new functionality
- Run `cargo clippy` and `cargo fmt` before committing

## File Extensions

- `.nr` - NEURO source code files
- `.nrm` - NEURO model files (serialized neural networks)
- `.nrl` - NEURO library files (compiled modules)
- `.nrp` - NEURO package definitions

## Performance Targets

- **Arithmetic Operations**: 2-5x slower than C++ (vs 100x for Python)
- **Neural Network Training**: Within 10% of PyTorch C++ performance
- **Memory Usage**: 1-2x of equivalent C++ programs
- **Compilation Time**: Fast incremental compilation with caching

## License

NEURO is licensed under the [GNU General Public License v3.0](LICENSE).

This is alpha-stage software. See LICENSE for important disclaimers about production use.

## Acknowledgments

NEURO draws inspiration from:
- Rust (ownership semantics, type system)
- Python (syntax simplicity, AI ecosystem)
- Swift (language design philosophy)
- Mojo (AI-first design)

## Contact

For questions, issues, or discussions, please use GitHub Issues.

**Note**: This is an educational and research project. It is not affiliated with any commercial entity.
