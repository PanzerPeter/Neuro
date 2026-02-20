# NEURO Programming Language

**Status:** Alpha Development - Phase 1 Core MVP Complete (Not Production Ready)

[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)
[![Tests](https://img.shields.io/badge/tests-312%20passing-success.svg)](#)
[![Phase](https://img.shields.io/badge/phase-1%20100%25%20complete-success.svg)](#phase-1-core-mvp-100-complete-)

A modern, compiled programming language designed for high-performance development workflows. NEURO combines static typing, type inference, and native code generation via LLVM.

## What is NEURO?

NEURO is an Ahead-of-Time (AOT) compiled language built from the ground up for AI workloads. Unlike Python, which serves as an interpreted glue language for C++ libraries, NEURO compiles directly to native code via LLVM, eliminating runtime overhead while maintaining clean, expressive syntax.

### Key Features

- **Core Language Support**: Functions, variables, control flow, and expressions
- **Static Typing + Inference**: Primitive types with contextual numeric inference
- **LLVM Backend**: Native executable generation from `.nr` source files
- **Compiler Tooling**: `neurc check` and `neurc compile` workflows
- **Future AI Features**: Tensor operations, AD, and GPU features are planned for later phases

## Quick Example

```neuro
func add(a: i32, b: i32) -> i32 {
    a + b
}

func main() -> i32 {
    val result: i32 = add(5, 3)
    result
}
```

## Installation

**Note:** NEURO is in active development. Installation and tooling requirements may change between releases.

### Prerequisites

- Rust 1.70 or later
- LLVM 18.1.8 (full development package required on Windows)
- CMake 3.20+
- vcpkg (Windows only, for libxml2 dependency)
- MSVC 2022 or MinGW-w64 (Windows) / GCC or Clang (Unix)
- (Optional) CUDA Toolkit 12.0+ for GPU support

### Building from Source

#### Windows Installation

```powershell
# 1. Install LLVM 18.1.8 (full development package)
# Download: clang+llvm-18.1.8-x86_64-pc-windows-msvc.tar.xz
# From: https://github.com/llvm/llvm-project/releases/tag/llvmorg-18.1.8
# Extract to: C:\LLVM-1818

# 2. Set environment variable
[System.Environment]::SetEnvironmentVariable('LLVM_SYS_181_PREFIX', 'C:\LLVM-1818', 'Machine')

# 3. Add LLVM to PATH
$machinePath = [System.Environment]::GetEnvironmentVariable('Path', 'Machine')
[System.Environment]::SetEnvironmentVariable('Path', "$machinePath;C:\LLVM-1818\bin", 'Machine')

# 4. Install vcpkg and libxml2
cd C:\
git clone https://github.com/Microsoft/vcpkg.git
cd vcpkg
.\bootstrap-vcpkg.bat
.\vcpkg install libxml2:x64-windows-static
.\vcpkg integrate install

# 5. Clone and build the compiler
git clone https://github.com/PanzerPeter/Neuro.git
cd Neuro
cargo build --release

# 6. Run tests
cargo test -p lexical-analysis
cargo test -p syntax-parsing
cargo test -p semantic-analysis

# 7. Test the compiler
cargo run -p neurc -- check examples/hello.nr

# 8. Install the compiler (optional)
cargo install --path compiler/neurc
```

#### Unix/Linux/macOS Installation

```bash
# 1. Install LLVM 18 (via package manager)
# Ubuntu/Debian:
wget https://apt.llvm.org/llvm.sh
chmod +x llvm.sh
sudo ./llvm.sh 18

# macOS (Homebrew):
brew install llvm@18

# 2. Set environment variable
export LLVM_SYS_181_PREFIX=/usr/lib/llvm-18  # Adjust path as needed
echo 'export LLVM_SYS_181_PREFIX=/usr/lib/llvm-18' >> ~/.bashrc

# 3. Clone and build
git clone https://github.com/PanzerPeter/Neuro.git
cd Neuro
cargo build --release

# 4. Run tests
cargo test --all

# 5. Install the compiler
cargo install --path compiler/neurc
```

**Note**: The `.cargo/config.toml` file is pre-configured with the vcpkg library path for Windows. On Unix systems, this configuration is ignored.

## Phase 1 Core MVP Complete

NEURO has completed **Phase 1** (Roadmap v3.8) for the current core MVP scope.

The compiler can now compile programs end-to-end from source code to native executables with full type checking, mutable variable reassignment, extended integer types, expression-based returns, and **string types**.

### Current Capabilities (Phase 1)

- **Lexical Analysis** - Complete tokenizer with Unicode support (28 tests)
- **Syntax Parsing** - Expression parser (Pratt) and statement parser with assignment and while-loop support
- **Semantic Analysis** - Full type checking with mutability enforcement (39 tests)
- **Variable Reassignment** - Mutable variables with type-safe assignment (`mut x = 0; x = 10`)
- **Expression-Based Returns** - Implicit returns from trailing expressions (Rust-like syntax)
- **Extended Primitive Types** - All integer types: i8, i16, i32, i64, u8, u16, u32, u64, f32, f64, bool
- **Type Inference for Numeric Literals** - Contextual type inference with range validation (semantic analysis complete; LLVM backend integration deferred)
- **String Type** - Full string literal support with escape sequences (\n, \t, \", \\, \xNN, \u{NNNN})
- **LLVM Backend** - Signedness-aware code generation with string support (4 tests)
- **CLI Compiler** - `neurc check` validates syntax/types, `neurc compile` produces executables
- **End-to-End Integration** - Full pipeline from source to binary (36 tests)
- **312 Tests Passing** - Comprehensive coverage across all components

### Try It Now!

```bash
# Compile a simple NEURO program
cargo run -p neurc -- compile examples/milestone.nr

# Run the compiled executable
./examples/milestone.exe  # Windows
./examples/milestone      # Unix

# Check syntax and types without compiling
cargo run -p neurc -- check examples/hello.nr
```

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
Neuro/
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

// Explicit mutability with reassignment
mut counter: i32 = 0
counter = counter + 1  // Type-safe reassignment
counter = 42           // Can reassign multiple times

// Contextual numeric inference
val pi = 3.14159  // Inferred as f64
```

### Functions

```rust
// Explicit return (traditional style)
func add(a: i32, b: i32) -> i32 {
    return a + b
}

// Expression-based return 
// The last expression automatically becomes the return value
func multiply(a: i32, b: i32) -> i32 {
    a * b  // implicit return - no 'return' keyword needed
}

// Works with complex expressions
func calculate(x: i32, y: i32) -> i32 {
    val step1: i32 = x * 2
    val step2: i32 = y + 10
    step1 + step2  // trailing expression becomes return value
}
```

### Tensor Types

Planned for later phases:

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
- Check existing issues and current priorities
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

## VSCode Extension

NEURO includes syntax highlighting support for VSCode. Currently available for local installation (marketplace publication planned for future release).

### Local Installation

```bash
# Navigate to the extension directory
cd neuro-language-support

# Install vsce if not already installed
npm install -g @vscode/vsce

# Package the extension
vsce package

# Install the generated .vsix file in VSCode
# In VSCode: Extensions (Ctrl+Shift+X) → ... → Install from VSIX
# Select: neuro-language-support-1.0.0.vsix
```

**Features**:
- Syntax highlighting for `.nr` files
- Line and block comment toggling
- Auto-closing brackets, quotes, and parentheses
- Smart indentation

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
