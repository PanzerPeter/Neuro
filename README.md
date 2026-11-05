# Neuro Programming Language

> An AOT-compiled language for high-performance AI development.

<p align="center">
  <img src="assets/demo.gif" alt="neurc rejects a swapped matrix product at compile time, then the fixed program trains a model with @grad and predicts on the GPU" width="880">
</p>

[![License: Neuro Shared Source License v2.1](https://img.shields.io/badge/License-NSSL%20v2.1-blue.svg)](LICENSE)
[![Documentation](https://img.shields.io/badge/docs-neuro--lang.netlify.app-blue.svg)](https://neuro-lang.netlify.app)
[![LLVM](https://img.shields.io/badge/LLVM-23-blue.svg)](https://llvm.org/)
[![CI](https://github.com/PanzerPeter/Neuro/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/PanzerPeter/Neuro/actions/workflows/ci.yml)

**Status: alpha.** Breaking changes are expected. Per-phase status lives in one place, the
[Quick Roadmap](#quick-roadmap).

---

## What Neuro is

Neuro is a programming language for building AI. You write your program in a text file, and
Neuro's compiler, `neurc`, turns it into a standalone program that runs directly on your computer,
the way a program written in C or Rust does. You can hand that program to someone else, and they
can run it without installing Python.

The building blocks of AI code are part of the language, so you don't need add-on libraries to get
them. A tensor, the grid of numbers a model does its maths on and stores what it has learned in, is
a built-in type. Training a model means working out how each of those numbers should change so the
model makes fewer mistakes. Put `@grad` on a function and the compiler writes that calculation for
you. Put `@gpu` on a function and, on Linux, it runs on an NVIDIA or AMD graphics card.

The recording at the top of this page shows these pieces in
[one short program](examples/showcase/train_predict.nr). The compiler refuses a multiplication
whose tensor sizes don't fit, before anything runs. Once that line is fixed, the program builds into
a small native file, learns the right numbers, and makes a prediction on the GPU.

---

## Why Neuro

AI development runs on two languages: an interpreted one to write in, and C++ or CUDA underneath
for anything that has to be fast. Crossing that boundary is where performance and type safety are
lost. Neuro is one language on both sides of it.

- **Native code, no interpreter.** Compiled ahead of time through LLVM 23, with no bytecode VM and
  no global interpreter lock. On scalar compute-bound programs it lands in the same range as
  `clang++ -O3` and Rust; see [Performance](#performance).
- **Shapes checked by the compiler.** `Tensor<T, [d0, d1]>` carries its dimensions in the type, so
  a dimension mismatch is a compile error rather than an exception thrown ninety minutes into a
  training run.
- **Ownership without a garbage collector.** Move-by-default with `val`/`mut`, borrows, and
  deterministic destruction, so there is no collector pause in the middle of a training step and
  no shared mutable state to make parallel tensor code unsafe.

The full reasoning, including what Neuro deliberately is not, is in [DESIGN.md](DESIGN.md).

---

## Quick Example

A single perceptron with ReLU activation, using structs, `impl` blocks, instance methods,
if-expressions and implicit returns. [This file compiles and runs today.](examples/structs/neuron.nr)

```neuro
struct Neuron {
    weight: f64,
    bias: f64
}

impl Neuron {
    func new(weight: f64, bias: f64) -> Neuron {
        Neuron { weight: weight, bias: bias }
    }

    // ReLU activation: pass-through if positive, clamp to zero otherwise
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

    // Dead region: input too small to overcome the bias
    val dead = neuron.activate(0.0)    // 0.0 * 0.5 - 0.1 = -0.1, clamped to 0.0
    val dead_fires = neuron.is_active(0.0)
    println("input 0.0 -> {dead:.2}  fires: {dead_fires}")

    // Active region: strong enough input fires the neuron
    val active = neuron.activate(1.0)  // 1.0 * 0.5 - 0.1 = 0.4, passes through
    val active_fires = neuron.is_active(1.0)
    println("input 1.0 -> {active:.2}  fires: {active_fires}")

    // The dead region clamps to exactly zero.
    if dead > 0.0 { return 1 }

    // Scale the active output into the exit code so the result is observed.
    return (active * 10.0) as i32      // 0.4 * 10 = 4
}
```

```text
input 0.0 -> 0.00  fires: false
input 1.0 -> 0.40  fires: true
```

More programs, each pinned to its exact exit code and printed output, are in
[examples/](examples/); [examples/showcase/](examples/showcase/) holds the ones that combine
several features at once.

---

## Installation

| Requirement | Version | Notes |
|---|---|---|
| **Rust** | 1.98.1+ | Install via [rustup](https://rustup.rs/) |
| **LLVM 23 and MLIR 23** | 23.x with dev libraries, in one prefix | Per-platform commands below |
| **C linker** | any | `gcc` / `clang` on Linux and macOS, MSVC on Windows |

### 1. LLVM 23 and MLIR 23

This is the only step that differs between systems. Every build compiles the MLIR backend, so
MLIR must sit in the same prefix as LLVM, and three variables name that prefix. Put the `prefix=`
and `export` lines in your shell profile (`~/.bashrc`, `~/.zshrc`) so they survive a new terminal.

```bash
# Ubuntu / Debian
wget -qO- https://apt.llvm.org/llvm.sh | sudo bash -s -- 23
sudo apt-get install -y llvm-23-dev llvm-23-tools libpolly-23-dev libmlir-23-dev mlir-23-tools \
  libclang-23-dev libclang-common-23-dev lld-23 libzstd-dev zlib1g-dev
prefix=/usr/lib/llvm-23
export LIBCLANG_PATH=$prefix/lib LLVM_LINK_LLVM_DYLIB=1

# macOS (Homebrew)
brew install llvm
prefix="$(brew --prefix llvm)"

export LLVM_SYS_231_PREFIX=$prefix MLIR_SYS_230_PREFIX=$prefix TABLEGEN_230_PREFIX=$prefix
```

Arch Linux / CachyOS and Windows have no MLIR package that fits, so both build LLVM and MLIR from
source into one prefix. The recipes are in the [installation guide](docs/getting-started/installation.md),
and [troubleshooting](docs/guides/troubleshooting.md) covers the errors that follow from getting
it wrong.

### 2. Build

```bash
git clone https://github.com/PanzerPeter/Neuro.git
cd Neuro
cargo build --release
cargo test --workspace

cargo install --path compiler/neurc   # optional, puts neurc on your PATH
```

The same four commands run unchanged in PowerShell on Windows.

### 3. Run something

```bash
neurc check examples/basics/hello.nr        # type-check only, no binary
neurc run   examples/basics/factorial.nr    # compile and run, leaving no binary behind
neurc compile examples/basics/factorial.nr  # native executable next to the source
```

Without `cargo install`, prefix each command with `cargo run -p neurc --`. Flags, `--emit obj` and
the zero-copy NumPy recipe are in the [CLI guide](docs/guides/cli-usage.md).

---

## Current Capabilities

Every row is implemented, tested and usable today. Depth lives in the
[documentation site](https://neuro-lang.netlify.app/) and [docs/](docs/); per-release detail is in
[CHANGELOG.md](CHANGELOG.md).

| Feature | Summary |
|---|---|
| **Types and inference** | `i8`-`u64`, `f16` / `bf16` / `f32` / `f64`, `bool`, `char`, `string`; literal suffixes, digit separators, `as` casts, aliases ([reference](docs/language-reference/types.md)) |
| **Control flow** | `if`, `while`, `loop`, `for` over ranges, collections and the iterator protocol, `'label` breaks, blocks as values ([reference](docs/language-reference/control-flow.md)) |
| **Functions and closures** | Named arguments, implicit returns, `\|>` pipelines, `>>` composition; closures that capture by copy, borrow or `move` ([reference](docs/language-reference/functions.md)) |
| **Generics and traits** | Monomorphized generics with const parameters and `where` clauses, associated types, operator traits, `impl` / `dyn` dispatch ([reference](docs/language-reference/generics-and-traits.md)) |
| **Structs and enums** | Methods with `&self` / `&mut self` / `self` receivers, payload variants, newtypes, `@derive(Copy, Clone, Debug, PartialEq, Hashable)` ([reference](docs/language-reference/structs.md)) |
| **Pattern matching** | Exhaustive `match` with nested, binding-or, `@`, array, range and guard patterns, `if val` / `while val`, `val-else`, destructuring `val` bindings |
| **Arrays and collections** | `[T; N]`, tuples, slices `&[T]` / `&mut [T]`, and `Vec<T>` / `HashMap<K, V>` / `BTreeMap<K, V>` owning any sized element |
| **Tensors** | `Tensor<T, [dims]>` with compile-time shapes: broadcasting, `@`, slicing, reductions, sorting, `einsum`, named and dynamic axes ([reference](docs/language-reference/tensors.md)) |
| **GPU** | `@gpu` functions as NVIDIA or AMD kernels on Linux with an opt-in CPU fallback, hand-written `@kernel`s, tensors kept on a GPU with `.to(...)` |
| **Automatic differentiation** | `@grad` compiles a reverse-mode derivative with no tape: `wrt:`, `.backward()`, `order: 2` Hessians, `.detach()` ([reference](docs/language-reference/autodiff.md)) |
| **Strings** | Immutable `string` slices, interpolation `"{x:.2}"`, triple-quoted blocks, codepoint iteration, `.parse::<T>()`, growable `StringBuilder` ([reference](docs/language-reference/strings.md)) |
| **Errors** | `Option<T>` / `Result<T, E>` in the prelude, `??` fallbacks, `?` propagation, `checked_*` arithmetic, `.to_checked::<T>()` conversions |
| **Ownership** | Moves, `Copy`, borrow exclusivity, references in fields, lifetimes, deterministic `Drop`, `pool { }` arenas ([reference](docs/language-reference/ownership.md)) |
| **Modules and toolchain** | File modules, `import` and re-exports, the prelude; `neurc check` / `run` / `compile` on LLVM 23, `--emit obj` for C and NumPy ([reference](docs/language-reference/modules.md)) |

> **Alpha memory note.** Memory is reclaimed deterministically, except in the handful of positions
> whose owner the compiler cannot prove, which leak a buffer rather than risk a dangling one. The
> [memory model](docs/language-reference/memory-model.md#what-still-leaks) lists them, and this note
> goes when that list is empty.

---

## Performance

`neurc compile -O 3` runs LLVM's `-O3` pipeline, the same level the C++ and Rust rows are
built at (`clang++ -O3`, `rustc -C opt-level=3`), and all three target the generic x86-64 CPU
with no `-march=native`. The default is `-O 0`, checked arithmetic with no optimization, so pass
`-O 3` before drawing any conclusion about speed.

Best of nine runs on one machine (Core i5-14600K, RTX 5070), lower is better. Reproduce with
`cd benchmarks && uv run python run.py --reps 9 --levels 3`, which builds every implementation
of each program and refuses to report timings if they disagree on output. Each one is written
the way a programmer of that language would write it for speed. The Python column is the
language itself; the NumPy column is what a Python programmer would write where the work
vectorizes.

| Benchmark | What it stresses | Neuro `-O 3` | `clang++ -O3` | `rustc -O3` | Python 3.14 | NumPy |
|---|---|---|---|---|---|---|
| `mandelbrot` | scalar `f64` in a tight loop | 166 ms | 166 ms | 166 ms | 6668 ms | 1268 ms |
| `vector_sum` | `Vec` push, sweep by reference | 24 ms | 26 ms | 24 ms | 6008 ms | 130 ms |
| `call_overhead` | recursion, call and inline cost | 49 ms | 51 ms | 48 ms | 1409 ms | |
| `print_lines` | integer holes to standard output | 13 ms | 20 ms | 13 ms | 111 ms | |
| `format_floats` | `f64` holes at a fixed precision | 115 ms | 110 ms | 38 ms | 222 ms | |
| `int_divide` | guarded `/` and `%`, opaque divisor | 95 ms | 88 ms | 89 ms | 1515 ms | |
| `matmul` | `@` on `[256, 256]` `f32` tensors | 3.5 ms | 3.9 ms | 3.8 ms | 770 ms | 61 ms |

Absolute times belong to the machine. Four rows are worth a word. `print_lines` beats C because
an integer hole renders through a digit loop instead of `snprintf`, as Rust's does.
`format_floats` loses to Rust, whose own float formatter is three times faster than the C
library conversion Neuro and C++ both call. `int_divide` is the one place the compiler spends:
`/` and `%` guard the operand pairs the hardware leaves undefined, and Rust guards the same pairs
for 7% less. `matmul` runs each `@` as a register-blocked, vectorized loop nest, level with the
i-k-j loops the C++ and Rust versions vectorize, and with the same bits as a plain loop.

The GPU benchmarks run their Neuro side as `@gpu` kernels, against PyTorch on the same GPU and
NumPy on the CPU. Each moves the same data between host and device:

| Benchmark | What it stresses | Neuro `@gpu` | PyTorch (CUDA) | NumPy (CPU) |
|---|---|---|---|---|
| `gpu_relax` | 200 fused element-wise steps on `[2048, 2048]` | 262 ms | 1080 ms | 619 ms |
| `gpu_reduce` | 1000 rounds of a whole and a row `.sum()` on `[2048, 2048]` | 747 ms | 1144 ms | 1251 ms |
| `gpu_matmul` | five `[2048, 2048]` `f32` products | 292 ms | 1137 ms | 217 ms |
| `gpu_mlp` | twenty batches through a two-layer perceptron | 275 ms | 1133 ms | 158 ms |

These are whole-program times, and both GPU columns start with a fixed cost: about 220 ms for a
Neuro program, most of it the CUDA driver starting up, and about 1100 ms for PyTorch's import and
CUDA setup. Past that, timed warm inside one process, PyTorch takes 23, 56, 37 and 13 ms against
Neuro's roughly 44, 530, 40 and 25. Neuro's GPU results keep the host's exact bits, and there is no
shared-memory staging or tensor-core path yet; [benchmarks/](benchmarks/README.md#reading-the-results)
has the detail.

---

## Quick Roadmap

Each numbered phase is a MAJOR-version milestone: completing **Phase N** ships **v(N+1).0.0**.

| Phase | Goal | Status |
|:---:|---|:---:|
| **1** | **Core Language**: types, control flow, LLVM backend, ownership and borrow checking, generics, traits, closures, enums and pattern matching, error handling, modules | Complete |
| **2** | **Tensors and MLIR**: first-class tensor types lowered through MLIR Linalg, the pool allocator, and the value model they need | Complete |
| **3** | **Automatic differentiation**: a reverse-mode source-to-source transform over Neuro's own typed HIR, `@grad(wrt: ...)`, `.backward()` / `.zero_grad()`, higher-order derivatives, elementwise math, `.detach()` / `@no_grad` | Complete |
| **4** | **GPU acceleration**: MLIR GPU dialects for NVIDIA and AMD, `@gpu` with an opt-in CPU fallback, `@kernel` and the `KernelOut<T>` aliasing model, device tensors on any GPU | Complete |
| **5** | **Language and backend completion**: MLIR as the only tensor backend, generic methods, closure capture modes, references in structs, tensors generic over their element type, named dynamic extents, a standard library written in Neuro, `@test` | In progress |
| **6** | **Neural network library**: typed gradients from `grad(...)`, `@model`, optimizers, layers and attention, `.safetensors` weights, a `DataLoader`, training benchmarks against PyTorch | Planned |
| **7** | **Parallelism and scale**: scoped `parallel { }` tasks, channels, multithreaded host kernels, multi-GPU data parallel | Planned |
| **8** | **Interop and deployment**: Python extension modules over DLPack, `extern func` foreign functions, `defer` | Planned |
| **9** | **Developer experience**: debug info, incremental compilation, Language Server Protocol, error recovery, formatter | Planned |
| **10** | **Distribution and optimization**: the `neurpm` package manager, prebuilt binaries and installers, a self-updater, optimization passes | Planned |

---

## Architecture

Neuro follows [Vertical Slice Architecture](VSA.md): code is organized by language feature, not by
technical layer.

```text
compiler/
├── infrastructure/          # Shared, zero-business-logic crates
│   ├── ast-types/           #   AST node definitions
│   ├── shared-types/        #   Primitives shared across slices
│   └── neuro-hir/           #   Typed High-Level IR (frontend <-> backend contract)
├── lexical-analysis/        # Tokenizer (logos, Unicode XID)
├── syntax-parsing/          # Pratt + statement parser -> AST
├── module-resolution/       # Multi-file loading, imports, visibility
├── argument-binding/        # Named arguments -> positional calls
├── semantic-analysis/       # Type checker, scope and borrow analysis
├── hir-lowering/            # Type-checked AST -> typed HIR
├── llvm-backend/            # HIR -> object code (inkwell / LLVM 23)
├── mlir-backend/            # HIR -> MLIR linalg and GPU kernels
└── neurc/                   # CLI compiler driver
```

Today a `.nr` file travels: **tokens → AST → merged program → type-checked AST → typed HIR →
LLVM object code → system linker**. Automatic differentiation runs inside HIR lowering. The
`mlir-backend` slice lowers the same typed HIR to MLIR `linalg`; `neurc` links the tensor
bodies it computes into the program, and on Linux lowers `@gpu` and `@kernel` bodies on through the
MLIR GPU dialects to NVIDIA or AMD kernels. Stage by stage:
[docs/compiler/compilation.md](docs/compiler/compilation.md).

---

## Documentation

| | |
|---|---|
| [Getting started](docs/getting-started/quick-start.md) | Installation, first program, workflow |
| [Language reference](docs/README.md#language-reference) | One page per feature area, from types to modules |
| [CLI guide](docs/guides/cli-usage.md) | Commands, flags, environment variables, interop |
| [Compiler internals](docs/README.md#compiler-architecture) | Pipeline and one page per slice |
| [Editor support](docs/guides/editor-support.md) | Syntax highlighting for `.nr` files |
| [DESIGN.md](DESIGN.md) | Why the language is shaped this way, and its non-goals |
| [CHANGELOG.md](CHANGELOG.md) | What each release changed |

Everything is published at [neuro-lang.netlify.app](https://neuro-lang.netlify.app).

---

## Contributing

[CONTRIBUTING.md](CONTRIBUTING.md) has the architecture rules, coding standards, quality gates and
pull request process. Open defects are in [docs/BUGS.md](docs/BUGS.md), and fixing one is the best
way to start. Work is most useful in the phase the [Quick Roadmap](#quick-roadmap) marks in progress.

See also [SECURITY.md](SECURITY.md) and the [Code of Conduct](CODE_OF_CONDUCT.md).

---

## License

[Neuro Shared Source License v2.1](LICENSE). The license covers the compiler, not what you build
with it.

You may freely use, study and modify the compiler for any personal or internal purpose, write
Neuro programs and distribute or sell the compiled output under any terms you choose, build tools
and editor integrations that call into the compiler, and contribute code back. Redistributing the
compiler itself, or a fork of it, as part of a commercial product requires a commercial license,
unless the fork is renamed and fully source-available under section 3.6 of the license.

Neuro is pre-stabilization, and the license guards three risks specific to that: commercial
re-packaging before the spec is stable, AI-assisted reproduction of the compiler for a competing
product, and misleading forks that fragment an early ecosystem. A permissive license becomes
possible once the language stabilizes.

## Acknowledgments

Inspired by Rust (ownership, type system), Python (AI ecosystem simplicity), Swift (ergonomics)
and Mojo (AI-first design). Built with [inkwell](https://github.com/TheDan64/inkwell),
[melior](https://github.com/mlir-rs/melior), [logos](https://github.com/maciejhirsz/logos),
[LLVM](https://llvm.org/) and [MLIR](https://mlir.llvm.org/).
