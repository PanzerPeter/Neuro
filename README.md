# Neuro Programming Language

> A modern, compiled language designed for high-performance AI development.

<p align="center">
  <img src="assets/demo.gif" alt="Compile and run a Neuro program in under a second" width="820">
</p>

[![License: Neuro Shared Source License v2.1](https://img.shields.io/badge/License-NSSL%20v2.1-blue.svg)](LICENSE)
[![LLVM](https://img.shields.io/badge/LLVM-20-blue.svg)](https://llvm.org/)
[![Tests](https://img.shields.io/badge/tests-784%20passing-success.svg)](#)

**Status:** Alpha — Phase 1 (Core Language) in progress · sub-phases 1A–1D complete (MVP, syntax & semantics, ownership/borrow checker, HIR & MLIR plumbing) · 1E (type system) active · → v2.0.0 when Phase 1 completes

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
- [Security](SECURITY.md)
- [Code of Conduct](CODE_OF_CONDUCT.md)
- [Design Rationale](DESIGN.md)

---

## Overview

Neuro is an Ahead-of-Time (AOT) compiled language built from the ground up for AI workloads. Unlike Python — an interpreted glue language — Neuro generates native code through an LLVM 20 backend, with a roadmap toward:

- **MLIR-based tensor operations** for static, shape-verified tensor types
- **IR-level automatic differentiation** via Enzyme
- **GPU acceleration** via MLIR GPU dialects (nvgpu, rocdl, Triton)

---

## Quick Example

A single perceptron with ReLU activation; uses structs, `impl` blocks, associated functions, instance methods, if-expressions, and implicit returns. [This file compiles and runs today.](examples/structs/neuron.nr)

```neuro
struct Neuron {
    weight: f64,
    bias: f64
}

impl Neuron {
    func new(weight: f64, bias: f64) -> Neuron {
        Neuron { weight: weight, bias: bias }
    }

    // ReLU: pass-through if active, clamp to zero if not
    func activate(&self, input: f64) -> f64 {
        val z = (input * self.weight) + self.bias
        if z > 0.0 { z } else { 0.0 }
    }

    func is_active(&self, input: f64) -> bool {
        val z = (input * self.weight) + self.bias
        z > 0.0
    }
}

func main() -> i32 {
    val neuron = Neuron::new(0.5, -0.1)

    val dead   = neuron.activate(0.0)   // 0.0 * 0.5 − 0.1 = −0.1 → clamped to 0.0
    val active = neuron.activate(1.0)   // 1.0 * 0.5 − 0.1 =  0.4 → passes through
    val fired  = neuron.is_active(1.0)  // true

    return 0
}
```

---

## Current Capabilities

Phase 1 (Core Language) sub-phases 1A–1D are complete; 1E (type system) is active. The following features are fully implemented and tested (**784 Tests Passing**):

| Feature | Details |
|---|---|
| **Static Typing + Inference** | All integer types (i8–u64), f16/bf16/f32/f64, bool, char, string; explicit `as` casting; contextual literal inference; literal type suffixes (`42i64`, `1.5f32`) and digit separators (`1_000_000`) |
| **Functions** | Parameters, explicit and expression-based implicit returns, recursion, forward references |
| **Control Flow** | if/else/elif, `while`, `loop` (incl. as a value expression), range-for (`0..n`, `0..=n`), `break`/`continue` with value-carrying breaks and loop labels |
| **Variables & Constants** | `val` (immutable) / `mut` (mutable) with type-safe reassignment; module- and function-scope `const` emitted as LLVM globals |
| **Structs & Methods** | Definition, instantiation, field-init shorthand, functional update (`..base`), field read/mutation; `impl` blocks with `&self` / `&mut self` instance methods and `TypeName::func` associated functions |
| **Arrays** | Fixed-size `[T; N]` of `Copy` scalars: literals with inference, index read/write, `.len()`, `for x in arr` / `for x in &arr`; debug-build out-of-bounds panic |
| **Tuples** | Anonymous `(T1, T2, ...)` of `Copy` elements: literals, `.0`/`.1` index access, destructuring `val (a, b) = t` with `_` wildcards and nesting; usable as function parameters and return types |
| **Destructuring** | Struct `val Point { x, y } = p` (field-name binds) and array `val [a, b, c] = arr` / `val [first, ..rest] = arr` (positional, with a trailing `..rest` remainder or bare `..`); arity-checked, nests, and works with `mut` |
| **Enums** | Tagged unions `enum E { A, B(i32), C { x: f64 } }` with unit, tuple, and struct-field variants (§3.5); construct via `E::A` / `E::B(1)` / `E::C { x: 1.0 }`; usable as bindings, function parameters/returns, and struct fields; `Copy`. Scalar payloads only; deconstruction (`match`) is the next 1E item |
| **Move Semantics** | Move-by-default for non-`Copy` values; use-after-move is a compile error; `.clone()` opts out; `@derive(Copy, Clone)` on structs |
| **Deterministic `Drop`** | `impl Drop for T { func drop(&mut self) }` runs a destructor at scope exit, in reverse declaration order, on normal exit only (never during a panic); a moved-out value is dropped exactly once; a `Copy` type may not implement `Drop` |
| **Borrows** | Immutable `&T` and mutable `&mut T` references with the `*` deref operator; flow-sensitive borrow exclusivity (shared XOR mutable) enforced at compile time |
| **Lifetimes (elision)** | Returned-reference lifetime elision; returning a borrow of a local or by-value parameter is rejected as it would dangle |
| **Strings** | Fat-pointer `string` with full escape support; `&string` borrowed slices; byte-level `==`/`!=`; `+` concatenation (heap-allocated new string); builtin `.len()` / `.clone()` / `.slice(a..b)` (zero-copy sub-slice, panics on out-of-bounds or mid-codepoint boundary) |
| **Panic Runtime** | `panic(msg)`, `assert(cond)`, `unreachable()` — print a located diagnostic to stderr and abort (no unwinding) |
| **LLVM Backend + CLI** | Native executable generation via inkwell 0.9.0 (LLVM 20); `neurc check` (type-check) and `neurc compile` (native binary) |
| **…and many more** | Half-precision `f16`/`bf16` scalars, `char`, type aliases, integer-overflow traps (debug) / wrapping (release), bitwise operators, compound assignment, if/block-as-value expressions, builtin integer methods (`wrapping_*`, `saturating_*`, `.shr`), attributes & lints, `unsafe` blocks. See [CHANGELOG.md](CHANGELOG.md) and [docs/](docs/) for the full list |

### Current Memory Model

> **⚠️ Alpha Memory Warning — no ownership system yet**
>
> Stack-allocated values (integers, booleans, structs with primitive fields) are reclaimed automatically on function return via LLVM `alloca`. String literals are emitted into read-only program memory (`.rodata`) and consume no heap, so a program that only reads literal strings does not leak today.
>
> The ownership system is now substantially in place: move-by-default, borrows, and **deterministic `Drop`** (user destructors run at scope exit) have landed. What remains is broader heap support — the growable-string builder and owning collections — plus full lifetime inference. Until those land, `+` string concatenation still leaks its heap buffer (it allocates a buffer no `Drop` impl yet frees), and runtime string builders do not exist.
>
> The ownership tracker, borrow checker, and deterministic destruction are sub-phase **1C** — now essentially complete (one flagged item, growable runtime strings, remains). Do not assume memory-safety semantics beyond what has actually landed.
>
> If memory safety semantics and compiler backend design are your thing, **[this is exactly where contributors are needed](CONTRIBUTING.md)**.

String fat pointers, move-by-default (use-after-move detection), the `Copy` trait, immutable borrows (`&T`), mutable borrows (`&mut T` with the `*` deref operator), flow-sensitive borrow exclusivity (the `&`/`&mut` aliasing rules), lifetime elision for returned references (§2.6), `&mut self` methods (in-place receiver mutation, §2.5), and deterministic `Drop` (scope-exit destructors, §2.1) have already landed; the remaining work — explicit lifetime annotations (scheduled with generics in 1F) and the growable runtime-string / owning-collection heap types (1G) — is tracked in the roadmap.

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
$version = "20.1.8"
$url = "https://github.com/llvm/llvm-project/releases/download/llvmorg-$version/LLVM-$version-win64.exe"
curl.exe -fsSL -o "$env:TEMP\llvm-installer.exe" $url
Start-Process "$env:TEMP\llvm-installer.exe" -ArgumentList "/S /D=C:\LLVM" -Wait -PassThru | Out-Null
```

Or download and run the installer manually from the
[LLVM GitHub releases page](https://github.com/llvm/llvm-project/releases) — install to `C:\LLVM`
(the path must not contain spaces; the NSIS installer enforces this).

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
cargo run -p neurc -- check examples/basics/hello.nr

# Compile to a native executable
cargo run -p neurc -- compile examples/basics/factorial.nr

# Run the compiled binary
./examples/factorial

# After cargo install --path compiler/neurc:
neurc compile examples/basics/factorial.nr
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

### Planned (Phase 2+): Tensor Types

Tensor types and compile-time shape verification require the MLIR lowering infrastructure planned for Phase 2. Shape constraints (`[784, 128]`) are encoded as static type parameters and verified at compile time via the MLIR type system — this is not a simple feature and depends on both the `melior` bindings (landed) and the typed High-Level IR (`neuro-hir`, landed in sub-phase 1D).

```neuro
// Static tensor — shape verified at compile time (Phase 2)
val weights: Tensor<f32, [784, 128]> = ...

// Automatic differentiation via Enzyme MLIR (Phase 3)
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
├── control-flow/            # CFG builder (not yet active)
├── llvm-backend/            # inkwell 0.9 / LLVM 20 codegen
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

**Planned extension (Phase 2+):**
```
Tensor/AI path: AST → Neuro High-Level IR
  → MLIR (linalg/tensor/func/arith, LLVM 20 / MLIR 20)
  → Enzyme MLIR AD pass (@grad)
  → GPU dialects (nvgpu/rocdl/Triton) or llvm dialect
  → inkwell → native code
```

---

## Quick Roadmap

Each numbered phase is a MAJOR-version milestone: completing **Phase N** ships **v(N+1).0.0**. We are in **Phase 1** (v1.x), divided into lettered sub-phases.

| Phase | Goal | Status |
|:---:|---|:---:|
| **1** | **Core Language** — the full general-purpose language; completing it ships **v2.0.0** | 🔄 In progress |
| 1A | Core MVP — types, functions, control flow, LLVM backend | ✅ Complete |
| 1B | Syntax & semantics stabilization — parser fixes, `const`, `as` casts, compound assignment, bitwise ops, integer suffixes, if/block expressions, `while true` lint, IEEE-754 float comparisons, string fat pointers | ✅ Complete |
| 1C | Ownership & borrow checker — move semantics, `Copy`, `&T`, `&mut T`, borrow exclusivity, lifetime elision / returned-reference outlives, `&mut self` methods, deterministic `Drop` | ✅ Complete ¹ |
| 1D | Backend plumbing — `neuro-hir` typed IR crate, `melior` integration, AST → HIR lowering, HIR-routed LLVM backend, mlir-backend HIR scaffold | ✅ Complete |
| 1E | Type system — arrays ✅, tuples ✅, structs ✅, methods ✅, destructuring ✅, type aliases ✅, enums ✅; pattern matching, newtype | 🔄 In progress |
| 1F | Generics, traits & dispatch — generics, explicit lifetimes, trait declarations, operator traits, static/dynamic dispatch (`impl`/`dyn`), closures | 📋 Planned |
| 1G | Error handling, modules & prelude — `Option`/`Result`, collections, `??`, `?`, multi-file modules, imports, prelude | 📋 Planned |
| 1H | Language cleanup — string interpolation, triple-quoted strings, nested comments, named arguments | 📋 Planned |
| **2** | Tensors & MLIR — `Tensor<T, [...]>`, shape generics, named dims, dynamic shapes, DLPack, MLIR linalg lowering, pool allocator, pipeline `|>`, composition `>>`, einstein notation | 📋 Planned |
| **3** | Automatic differentiation — Enzyme MLIR pass, `@grad(wrt: ...)`, `.backward()` / `.zero_grad()`, higher-order derivatives, SGD | 📋 Planned |
| **4** | GPU acceleration — MLIR GPU dialects (nvgpu / rocdl / Triton), `@gpu`, `KernelOut<T>` aliasing model, device memory pool, CPU fallback | 📋 Planned |
| **5** | Neural network standard library — `TrainableTensor`, `ParameterList`, optimizers, `@model`, Dense / Conv2d / Attention, `.nrm` serialization | 📋 Planned |
| **6** | Async runtime — `async func`, `Future<T>`, `spawn`, `JoinHandle`, `join` / `race`, executor for data-loader / I/O overlap | 📋 Planned |
| **7** | Interop & advanced features — Python FFI via DLPack, spread operator, advanced pattern matching, custom attributes, `defer` | 📋 Planned |
| **8** | Developer experience — Language Server Protocol, diagnostics polish, formatter, `@test` runner | 📋 Planned |
| **9** | Package manager & distribution — `neurpm`, cross-OS installer / uninstaller / self-updater, signed release binaries, optimization passes (loop unrolling, AD-aware inlining, LTO) | 📋 Planned |

¹ Sub-phase 1C is essentially complete; one flagged item (growable runtime strings) remains, with relocation to 1G pending sign-off.

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

The project is in early alpha — breaking changes are expected. Contributions should focus on **Phase 1 (Core Language)** items — currently sub-phase **1E** (type system).

---

## Why Neuro?

AI development is stuck in a fragmented paradigm: developers iterate in an interpreted glue language (Python), while underlying libraries are written in unmanaged, safety-critical systems languages (C++/CUDA). 

Neuro is built to unify this stack:
1. **True Native Performance:** Compiled AOT via LLVM 20—no heavy runtime interpreter, no global interpreter lock (GIL).
2. **AI-First Type System:** Native compile-time shape verification for tensors using MLIR (Phase 2), preventing runtime dimension mismatches before a single line of training executes.
3. **Immutability by Default:** A modern `val`/`mut` paradigm to ensure highly parallelized tensor computations are thread-safe by design.

---

## License

Licensed under the [Neuro Shared Source License v2.1](LICENSE).

**Why not MIT/Apache 2.0 right now?** Neuro is in a critical pre-stabilization phase. The license protects against three specific risks: commercial re-packaging of the compiler before the language spec is stable, AI-assisted reproduction of the compiler for a competing product, and misleading forks that fragment the early ecosystem. None of these restrictions affect normal use.

**What you can do freely:**
- Use, study, and modify the compiler for any personal or internal purpose
- Write Neuro programs and distribute or sell the compiled output under **any** terms you choose. programs you compile are wholly exempt from this license
- Build tools, plugins, and editor integrations that call into the compiler
- Contribute code back to the project

**What requires a commercial license:**
- Redistributing the Neuro compiler itself (or a fork of it) as part of a commercial product

See [LICENSE](LICENSE) for full terms.

## Acknowledgments

Inspired by Rust (ownership, type system), Python (AI ecosystem simplicity), Swift (language ergonomics), and Mojo (AI-first design). Built with [inkwell](https://github.com/TheDan64/inkwell), [logos](https://github.com/maciejhirsz/logos), and the [LLVM](https://llvm.org/) infrastructure.