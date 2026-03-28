# NEURO Programming Language

> A modern, compiled language designed for high-performance AI development.

[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org/)
[![LLVM](https://img.shields.io/badge/LLVM-20-blue.svg)](https://llvm.org/)
[![Tests](https://img.shields.io/badge/tests-348%20passing-success.svg)](#)

**Status:** Alpha — Phase 1 Core MVP complete · Phase 1.5 (backend upgrade + memory safety refactor) in progress

---

## Table of Contents

- [Overview](#overview)
- [Quick Example](#quick-example)
- [Current Capabilities](#current-capabilities)
- [Installation](#installation)
- [Usage](#usage)
- [Language Syntax](#language-syntax)
- [Architecture](#architecture)
- [Roadmap](#roadmap)
- [Development](#development)
- [VSCode Extension](#vscode-extension)
- [File Extensions](#file-extensions)
- [Contributing](#contributing)
- [License](#license)
- [Acknowledgments](#acknowledgments)

---

## Overview

NEURO is an Ahead-of-Time (AOT) compiled language built from the ground up for AI workloads. Unlike Python — an interpreted glue language — NEURO generates native code through an LLVM 20 backend, with a roadmap toward:

- **MLIR-based tensor operations** for static, shape-verified tensor types
- **IR-level automatic differentiation** via Enzyme
- **GPU acceleration** via MLIR GPU dialects (nvgpu, rocdl, Triton)

---

## Quick Example

```neuro
func factorial(n: i32) -> i32 {
    if n <= 1 {
        1
    } else {
        n * factorial(n - 1)
    }
}

func main() -> i32 {
    factorial(10)
}
```

---

## Current Capabilities

Phase 1 is complete. The following features are fully implemented and tested (**348 Tests Passing**):

| Feature | Details |
|---|---|
| **Static Typing + Inference** | All integer types (i8–u64), f32/f64, bool, string; contextual numeric literal inference |
| **Functions** | Parameters, explicit and expression-based implicit returns, recursion, forward references |
| **Control Flow** | if/else/elif, while loops, range-for (`for i in 0..n`), break, continue |
| **Mutable Variables** | `val` (immutable) and `mut` (mutable) with type-safe reassignment |
| **String Type** | Literals with full escape sequence support (`\n`, `\t`, `\"`, `\\`, `\xNN`, `\u{NNNN}`); `==` and `!=` for byte-level comparison |
| **Structs** | Definition, instantiation (`Name { field: value }`), field read (`obj.field`), field mutation on `mut` bindings; nominal typing; definition-order independent |
| **LLVM Backend** | Native executable generation via inkwell 0.8.0 (LLVM 20) |
| **CLI** | `neurc check` (type-check only) and `neurc compile` (produces native binary) |

---

## Installation

### Prerequisites

- **Rust** 1.85 or later
- **LLVM 20** with development libraries

---

### Arch Linux / CachyOS

```bash
# 1. Install LLVM 20
sudo pacman -S llvm20

# 2. Install Rust via rustup
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# 3. Set the LLVM prefix for the build system
export LLVM_SYS_201_PREFIX=/usr/lib/llvm20
# Add to ~/.bashrc or ~/.zshrc to make permanent

# 4. Clone and build
git clone https://github.com/PanzerPeter/Neuro.git
cd Neuro
cargo build --release

# 5. Run tests
cargo test --workspace

# 6. (Optional) Install the compiler globally
cargo install --path compiler/neurc
```

### Ubuntu / Debian

```bash
# 1. Install LLVM 20
wget https://apt.llvm.org/llvm.sh && chmod +x llvm.sh && sudo ./llvm.sh 20

# 2. Set the LLVM prefix
export LLVM_SYS_201_PREFIX=/usr/lib/llvm-20
echo 'export LLVM_SYS_201_PREFIX=/usr/lib/llvm-20' >> ~/.bashrc

# 3. Clone and build
git clone https://github.com/PanzerPeter/Neuro.git
cd Neuro
cargo build --release
```

### macOS (Homebrew)

```bash
# 1. Install LLVM 20
brew install llvm@20
export LLVM_SYS_201_PREFIX=$(brew --prefix llvm@20)
echo "export LLVM_SYS_201_PREFIX=$(brew --prefix llvm@20)" >> ~/.zshrc

# 2. Clone and build
git clone https://github.com/PanzerPeter/Neuro.git
cd Neuro
cargo build --release
```

---

## Usage

```bash
# Type-check a source file (no binary produced)
cargo run -p neurc -- check examples/hello.nr

# Compile to a native executable
cargo run -p neurc -- compile examples/factorial.nr

# Run the compiled binary
./examples/factorial

# After cargo install --path compiler/neurc:
neurc compile examples/factorial.nr
```

---

## Language Syntax

### Variables and Types

```neuro
// Immutable by default
val x: i32 = 42
val name: string = "NEURO"

// Mutable with reassignment
mut counter: i32 = 0
counter = counter + 1

// Type inference from context
val pi = 3.14159   // inferred f64
val n  = 100       // inferred i32
```

### Functions

```neuro
// Explicit return
func add(a: i32, b: i32) -> i32 {
    return a + b
}

// Expression-based implicit return (trailing expression)
func multiply(a: i32, b: i32) -> i32 {
    a * b
}
```

### Control Flow

```neuro
func fizzbuzz(n: i32) -> i32 {
    mut i: i32 = 1
    while i <= n {
        i = i + 1
    }
    i
}

func sum(n: i32) -> i32 {
    mut total: i32 = 0
    for i in 0..n {
        total = total + i
    }
    total
}
```

### Structs

```neuro
struct Point {
    x: f64,
    y: f64
}

func distance(p: Point) -> f64 {
    // field read
    val dx = p.x
    val dy = p.y
    dx * dx + dy * dy   // placeholder (no sqrt yet)
}

func main() -> i32 {
    val origin = Point { x: 0.0, y: 0.0 }

    // field mutation requires mut binding
    mut cursor = Point { x: 3.0, y: 4.0 }
    cursor.x = 1.0

    return 0
}
```

### Planned (Phase 3+): Tensor Types

```neuro
// Static tensor with compile-time shape verification
val weights: Tensor<f32, [784, 128]> = ...

// Automatic differentiation (Phase 4)
@grad(model) {
    val loss = model.forward(batch).cross_entropy(labels)
    model.backward(loss)
}
```

---

## Architecture

NEURO follows **Vertical Slice Architecture (VSA)** — organized by language feature, not technical layer.

### Workspace Layout

```
compiler/
├── infrastructure/          # Shared types, diagnostics, AST definitions
│   ├── ast-types/
│   ├── diagnostics/
│   ├── shared-types/
│   └── ...
├── lexical-analysis/        # Tokenizer (logos, Unicode XID)
├── syntax-parsing/          # Pratt + statement parser → AST
├── semantic-analysis/       # Type checker, scope analysis
├── control-flow/            # CFG builder (Phase 2+)
├── llvm-backend/            # inkwell 0.8 / LLVM 20 codegen
└── neurc/                   # CLI compiler driver
```

### Compilation Pipeline

**Current (Phase 1):**
```
Source (.nr)
  → Lexical Analysis   (tokens)
  → Syntax Parsing     (AST)
  → Semantic Analysis  (type-checked AST)
  → LLVM Backend       (object code via inkwell / LLVM 20)
  → System Linker      (native executable)
```

**Planned extension (Phase 3+):**
```
Tensor/AI path: AST → NEURO High-Level IR
  → MLIR (linalg/tensor/func/arith, LLVM 20 / MLIR 20)
  → Enzyme MLIR AD pass (@grad)
  → GPU dialects (nvgpu/rocdl/Triton) or llvm dialect
  → inkwell → native code
```

---

## Roadmap

| Phase | Goal | Status |
|:---:|---|:---:|
| **1** | Core MVP — types, functions, control flow, LLVM backend | ✅ Complete |
| **1.5** | Backend upgrade (LLVM 20), string fat pointers, ownership semantics | 🔄 In progress |
| **2** | Structs, enums, pattern matching, module system, error handling | 🔄 In progress (structs ✅) |
| **3** | Tensor types, MLIR lowering, DLPack, pool allocator | 📋 Planned |
| **4** | Automatic differentiation via Enzyme MLIR | 📋 Planned |
| **5** | GPU acceleration via MLIR GPU dialects (nvgpu/rocdl/Triton) | 📋 Planned |
| **6** | Neural network standard library, Python FFI via DLPack | 📋 Planned |

---

## Development

```bash
# Build the full workspace
LLVM_SYS_201_PREFIX=/usr/lib/llvm20 cargo build --workspace

# Run all tests
LLVM_SYS_201_PREFIX=/usr/lib/llvm20 cargo test --workspace

# Lint
cargo clippy --workspace --all-targets -- -D warnings

# Format
cargo fmt --all
```

---

## VSCode Extension

Syntax highlighting for `.nr` files is included in `neuro-language-support/`.

```bash
cd neuro-language-support
npm install -g @vscode/vsce
vsce package
# Install the generated .vsix via: VSCode → Extensions → Install from VSIX
```

---

## File Extensions

| Extension | Purpose |
|---|---|
| `.nr` | NEURO source files |
| `.nrl` | Compiled library modules |
| `.nrm` | Serialized model/matrix data |
| `.nrp` | Package definitions |

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for architecture guidelines, coding standards, and the pull request process.

The project is in early alpha — breaking changes are expected. Contributions should focus on **Phase 1.5** and **Phase 2** items.

---

## License

[GNU General Public License v3.0](LICENSE). Alpha-stage software — not production ready.

## Acknowledgments

Inspired by Rust (ownership, type system), Python (AI ecosystem simplicity), Swift (language ergonomics), and Mojo (AI-first design). Built with [inkwell](https://github.com/TheDan64/inkwell), [logos](https://github.com/maciejhirsz/logos), and the [LLVM](https://llvm.org/) infrastructure.