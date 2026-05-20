# Neuro Programming Language

> A modern, compiled language designed for high-performance AI development.

[![License: Neuro Source-Available License](https://img.shields.io/badge/License-NEURO%20Source--Available-blue.svg)](LICENSE)
[![LLVM](https://img.shields.io/badge/LLVM-20-blue.svg)](https://llvm.org/)
[![Tests](https://img.shields.io/badge/tests-452%20passing-success.svg)](#)

**Status:** Alpha — Phase 1 Core MVP complete · Phase 1.5 & Phase 2 in progress

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

Neuro is an Ahead-of-Time (AOT) compiled language built from the ground up for AI workloads. Unlike Python — an interpreted glue language — Neuro generates native code through an LLVM 20 backend, with a roadmap toward:

- **MLIR-based tensor operations** for static, shape-verified tensor types
- **IR-level automatic differentiation** via Enzyme
- **GPU acceleration** via MLIR GPU dialects (nvgpu, rocdl, Triton)

---

## Quick Example

```neuro
func factorial(n: i32) -> i32 {
    if n <= 1 {
        return 1
    }
    return n * factorial(n - 1)
}

func main() -> i32 {
    factorial(10)
}
```

---

## Current Capabilities

Phase 1 is complete and Phase 2 is in progress. The following features are fully implemented and tested (**452 Tests Passing**):

| Feature | Details |
|---|---|
| **Static Typing + Inference** | All integer types (i8–u64), f32/f64, bool, string; explicit `as` casting; contextual numeric literal inference; integer literal type suffixes (`42i64`, `255u8`) |
| **Functions** | Parameters, explicit and expression-based implicit returns, recursion, forward references |
| **Control Flow** | if/else/elif, while loops, range-for (`for i in 0..n` and `0..=n`), break, continue |
| **Mutable Variables** | `val` (immutable) and `mut` (mutable) with type-safe reassignment |
| **Constants** | `const NAME: Type = expr` at module and function scope; constant-expression validation; forward references; emitted as LLVM globals |
| **Bitwise Operators** | `&`, `\|`, `^`, `~`, `<<` on integer types; correct precedence per Appendix B (Shl > BitAnd > BitXor > BitOr); floats and bools rejected |
| **String Type** | Literals with full escape sequence support (`\n`, `\t`, `\"`, `\\`, `\xNN`, `\u{NNNN}`); `==` and `!=` for byte-level comparison |
| **Structs** | Definition, instantiation (`Name { field: value }`), field read (`obj.field`), field mutation on `mut` bindings; nominal typing; definition-order independent |
| **Methods** | `impl` blocks with `&self` instance methods; associated functions called via `TypeName::func(args)`; `&mut self` / consuming `self` rejected until ownership lands |
| **If/Block Expressions** | `val x = if cond { a } else { b }`; `val y = { stmts; expr }`; all arms type-checked; alloca-based lowering |
| **LLVM Backend** | Native executable generation via inkwell 0.8.0 (LLVM 20) |
| **CLI** | `neurc check` (type-check only) and `neurc compile` (produces native binary) |

### Current Memory Model

No ownership or destructor system exists yet. Stack-allocated values (integers, booleans, structs with no heap fields) are reclaimed automatically on function return via LLVM's `alloca`. Heap-allocated data — the backing buffer of every `string` value — is currently **leaked**. This is a known limitation of the alpha and is the primary motivation for Phase 1.5's ownership and borrow checker work. Do not use the current compiler for production workloads that allocate unbounded strings.

---

## Installation

### Prerequisites

| Requirement | Version | Notes |
|---|---|---|
| **Rust** | 1.85+ | Install via [rustup](https://rustup.rs/) |
| **LLVM 20** | 20.x with dev libs | Platform instructions below |
| **C linker** | any | `gcc`/`clang` on Linux/macOS; MSVC on Windows |

---

### Arch Linux / CachyOS

```bash
# 1. Install LLVM 20
sudo pacman -S llvm20

# 2. Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# 3. Set the LLVM prefix (add to ~/.bashrc or ~/.zshrc to persist)
export LLVM_SYS_201_PREFIX=/usr/lib/llvm20

# 4. Clone and build
git clone https://github.com/PanzerPeter/Neuro.git
cd Neuro
cargo build --release

# 5. Run the test suite
cargo test --workspace

# 6. (Optional) Install the compiler globally
cargo install --path compiler/neurc
```

### Ubuntu / Debian

```bash
# 1. Install LLVM 20 via the official APT script
wget -qO- https://apt.llvm.org/llvm.sh | sudo bash -s -- 20
# Alternatively, use the full dev package set:
# sudo apt-get install llvm-20 llvm-20-dev llvm-20-tools libpolly-20-dev

# 2. Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# 3. Set the LLVM prefix (add to ~/.bashrc to persist)
export LLVM_SYS_201_PREFIX=/usr/lib/llvm-20
echo 'export LLVM_SYS_201_PREFIX=/usr/lib/llvm-20' >> ~/.bashrc

# 4. Clone and build
git clone https://github.com/PanzerPeter/Neuro.git
cd Neuro
cargo build --release

# 5. Run the test suite
cargo test --workspace

# 6. (Optional) Install the compiler globally
cargo install --path compiler/neurc
```

### macOS (Homebrew)

```bash
# 1. Install LLVM 20
brew install llvm@20

# 2. Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# 3. Set the LLVM prefix (add to ~/.zshrc or ~/.bash_profile to persist)
export LLVM_SYS_201_PREFIX="$(brew --prefix llvm@20)"
echo "export LLVM_SYS_201_PREFIX=$(brew --prefix llvm@20)" >> ~/.zshrc

# 4. Clone and build
git clone https://github.com/PanzerPeter/Neuro.git
cd Neuro
cargo build --release

# 5. Run the test suite
cargo test --workspace

# 6. (Optional) Install the compiler globally
cargo install --path compiler/neurc
```

### Windows 10 / 11 (x64)

Windows requires the **MSVC toolchain** (not GNU). Make sure Visual Studio Build
Tools 2019 or later are installed with the **C++ build tools** workload before
proceeding.

**Step 1 — Install Visual Studio Build Tools**

Download from [visualstudio.microsoft.com/downloads](https://visualstudio.microsoft.com/downloads/)
→ *Tools for Visual Studio* → *Build Tools for Visual Studio 2022*.
Select the **Desktop development with C++** workload.

**Step 2 — Install Rust**

Download and run `rustup-init.exe` from [rustup.rs](https://rustup.rs/).
When prompted, choose *1) Proceed with standard installation*. Rustup will
automatically select the `stable-x86_64-pc-windows-msvc` default toolchain.

Open a **new** PowerShell window after installation so the `cargo` and `rustc`
commands are on your `PATH`.

**Step 3 — Install LLVM 20**

Download the official Windows installer from the LLVM GitHub releases page:

```powershell
# PowerShell — download and run the installer silently
$version = "20.1.4"
$url = "https://github.com/llvm/llvm-project/releases/download/llvmorg-$version/LLVM-$version-win64.exe"
Invoke-WebRequest -Uri $url -OutFile "$env:TEMP\llvm-installer.exe" -UseBasicParsing
Start-Process "$env:TEMP\llvm-installer.exe" -ArgumentList "/S /D=C:\LLVM" -Wait
```

Or download and run the installer manually — install to `C:\LLVM` (or any path
without spaces).

**Step 4 — Set the LLVM environment variable**

```powershell
# Set permanently for your user account (no admin required)
[Environment]::SetEnvironmentVariable(
    "LLVM_SYS_201_PREFIX", "C:\LLVM",
    [EnvironmentVariableTarget]::User
)
# Also add C:\LLVM\bin to your PATH
$current = [Environment]::GetEnvironmentVariable("Path", "User")
[Environment]::SetEnvironmentVariable("Path", "$current;C:\LLVM\bin", "User")
```

Close and reopen PowerShell so the changes take effect, then verify:

```powershell
llvm-config --version   # should print 20.x.y
```

**Step 5 — Clone and build**

```powershell
git clone https://github.com/PanzerPeter/Neuro.git
cd Neuro
cargo build --release
```

**Step 6 — Run the test suite**

```powershell
cargo test --workspace
```

**Step 7 — (Optional) Install the compiler globally**

```powershell
cargo install --path compiler/neurc
# The binary is placed in %USERPROFILE%\.cargo\bin\neurc.exe
# which is already on PATH after rustup setup.
```

> **Troubleshooting Windows build errors**
>
> - *`llvm-sys` build script cannot find LLVM*: confirm `LLVM_SYS_201_PREFIX`
>   is set in the **current** shell session (`echo $env:LLVM_SYS_201_PREFIX`)
>   and points to a directory that contains `bin\llvm-config.exe`.
> - *`link.exe` not found*: the MSVC Build Tools are not on `PATH`. Run the
>   build from a **Developer PowerShell** / **x64 Native Tools Command Prompt**
>   or install the *C++ build tools* workload as described in Step 1.
> - *Version mismatch (`llvm-sys-201` requires LLVM 20)*: an older LLVM is on
>   `PATH`. Set `LLVM_SYS_201_PREFIX` explicitly to the LLVM 20 prefix and
>   ensure `C:\LLVM\bin` precedes any other LLVM entries in `PATH`.

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
val name: string = "Neuro"

// Mutable with reassignment
mut counter: i32 = 0
counter = counter + 1

// Type inference works for both val and mut
val pi = 3.14159   // inferred f64
val n  = 100       // inferred i32
mut count = 0      // inferred i32; type annotation optional
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

Tensor types and compile-time shape verification require the MLIR lowering infrastructure planned for Phase 3. Shape constraints (`[784, 128]`) are encoded as static type parameters and verified at compile time via the MLIR type system — this is not a simple feature and depends on both the `melior` bindings and a typed High-Level IR (`neuro-hir`) that does not yet exist.

```neuro
// Static tensor — shape verified at compile time (Phase 3)
val weights: Tensor<f32, [784, 128]> = ...

// Automatic differentiation via Enzyme MLIR (Phase 4)
@grad(model) {
    val loss = model.forward(batch).cross_entropy(labels)
    model.backward(loss)
}
```

---

## Architecture

Neuro follows **Vertical Slice Architecture (VSA)** — organized by language feature, not technical layer.

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
Tensor/AI path: AST → Neuro High-Level IR
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
| **1.5** | key updates, string fat pointers, ownership semantics | 🔄 In progress |
| **2** | Structs, enums, pattern matching, module system, error handling | 🔄 In progress (structs ✅, methods ✅) |
| **3** | Tensor types, MLIR lowering, DLPack, pool allocator | 📋 Planned |
| **4** | Automatic differentiation via Enzyme MLIR | 📋 Planned |
| **5** | GPU acceleration via MLIR GPU dialects (nvgpu/rocdl/Triton) | 📋 Planned |
| **6** | Neural network standard library, Python FFI via DLPack | 📋 Planned |

---

## Development

Set `LLVM_SYS_201_PREFIX` for your platform before running any Cargo command
(see [Installation](#installation) for the correct path per OS).

```bash
# Build the full workspace
cargo build --workspace

# Run all tests
cargo test --workspace

# Lint
cargo clippy --workspace --all-targets -- -D warnings

# Format check
cargo fmt --all -- --check

# Apply formatting
cargo fmt --all
```

On Windows, use PowerShell or a Developer Command Prompt. The env var must be
set in the current session; prefix it inline if needed:

```powershell
$env:LLVM_SYS_201_PREFIX = "C:\LLVM"
cargo build --workspace
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
| `.nr` | Neuro source files |
| `.nrl` | Compiled library modules |
| `.nrm` | Serialized model/matrix data |
| `.nrp` | Package definitions |

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for architecture guidelines, coding standards, and the pull request process.

The project is in early alpha — breaking changes are expected. Contributions should focus on **Phase 1.5** and **Phase 2** items.

---

## License

Licensed under the [Neuro Source-Available License](LICENSE).

This software is an Alpha release. The license includes mandatory redistribution terms to preserve attribution, enforce alpha-status disclosure, and limit liability (e.g., barring use in safety-critical deployments without acknowledgement). See [LICENSE](LICENSE) for the full breakdown and redistribution checklist.

## Acknowledgments

Inspired by Rust (ownership, type system), Python (AI ecosystem simplicity), Swift (language ergonomics), and Mojo (AI-first design). Built with [inkwell](https://github.com/TheDan64/inkwell), [logos](https://github.com/maciejhirsz/logos), and the [LLVM](https://llvm.org/) infrastructure.