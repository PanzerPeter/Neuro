# Neuro Programming Language

> An AOT-compiled language for high-performance AI development.

<p align="center">
  <img src="assets/demo.gif" alt="neurc type-checks, compiles, and runs a Neuro program in under a second" width="880">
</p>

[![License: Neuro Shared Source License v2.1](https://img.shields.io/badge/License-NSSL%20v2.1-blue.svg)](LICENSE)
[![Documentation](https://img.shields.io/badge/docs-neuro--lang.netlify.app-blue.svg)](https://neuro-lang.netlify.app)
[![LLVM](https://img.shields.io/badge/LLVM-20-blue.svg)](https://llvm.org/)
[![CI](https://github.com/PanzerPeter/Neuro/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/PanzerPeter/Neuro/actions/workflows/ci.yml)

**Status: alpha.** Phases 1 (Core Language) and 2 (Tensors and MLIR) are complete: the
general-purpose language surface and the tensor stack compile and run. Phase 3 (Automatic
Differentiation) is open. Breaking changes are expected. Per-phase status lives in one place,
the [Quick Roadmap](#quick-roadmap).

---

## Why Neuro

AI development runs on two languages: an interpreted one to write in, and C++ or CUDA underneath
for anything that has to be fast. Crossing that boundary is where performance and type safety are
lost. Neuro is one language on both sides of it.

- **Native code, no interpreter.** Compiled ahead of time through LLVM 20, with no bytecode VM and
  no global interpreter lock. On compute-bound programs it lands in the same range as
  `clang -O2`; see [Performance](#performance).
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

    val dead = neuron.activate(0.0)         // 0.0 * 0.5 - 0.1 = -0.1 -> clamped to 0.0
    val dead_fires = neuron.is_active(0.0)
    println("input 0.0 -> {dead:.2}  fires: {dead_fires}")

    val active = neuron.activate(1.0)       // 1.0 * 0.5 - 0.1 =  0.4 -> passes through
    val active_fires = neuron.is_active(1.0)
    println("input 1.0 -> {active:.2}  fires: {active_fires}")

    if dead > 0.0 { return 1 }

    return (active * 10.0) as i32           // 4
}
```

```
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
| **Rust** | 1.85+ | Install via [rustup](https://rustup.rs/) |
| **LLVM 20** | 20.x with dev libraries | Per-platform commands below |
| **C linker** | any | `gcc` / `clang` on Linux and macOS, MSVC on Windows |

### 1. LLVM 20

This is the only step that differs between systems. Put the `export` in your shell profile
(`~/.bashrc`, `~/.zshrc`) so it survives a new terminal.

```bash
# Arch Linux / CachyOS
sudo pacman -S llvm20
export LLVM_SYS_201_PREFIX=/usr/lib/llvm20

# Ubuntu / Debian
wget -qO- https://apt.llvm.org/llvm.sh | sudo bash -s -- 20
export LLVM_SYS_201_PREFIX=/usr/lib/llvm-20

# macOS (Homebrew)
brew install llvm@20
export LLVM_SYS_201_PREFIX="$(brew --prefix llvm@20)"
```

Windows needs the MSVC toolchain and a **full LLVM 20 development build**: the official installer
ships Clang and `LLVM-C.dll` but no `llvm-config.exe`, no headers and no static libraries, so
`llvm-sys` cannot build against it. The PowerShell walkthrough is in the
[installation guide](docs/getting-started/installation.md#windows-msvc), and
[troubleshooting](docs/guides/troubleshooting.md) covers the errors that follow from getting it
wrong.

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
| **Types and inference** | `i8` through `u64`, `f16` / `bf16` / `f32` / `f64`, `bool`, `char`, `string`; literal suffixes, digit separators, `as` casts, type aliases |
| **Control flow** | `if` / `elif` / `else`, `while`, `loop`, range-`for`, labelled `break` / `continue`, block-as-value, `for` over any type implementing the prelude's iterator protocol |
| **Functions** | Recursion, forward references, implicit returns, named arguments with external labels, higher-order functions, `\|>` pipelines, `>>` composition |
| **Generics and traits** | Generic functions, structs and impls, const generics, `where` clauses, turbofish; required and default methods, associated types, operator traits, `impl Trait` and `dyn Trait` dispatch. Fully monomorphized |
| **Closures** | `\|x: i32\| x * x`, `move` closures, `(T) -> R` function types, compiled to `{ fn_ptr, env_ptr }` with no heap allocation |
| **Structs, enums, newtypes** | Fields, functional update `..base`, `impl` blocks with `&self` / `&mut self` / consuming receivers; unit, tuple and struct-field variants carrying any sized payload; `@derive(Copy, Clone, Debug, PartialEq)` |
| **Pattern matching** | Exhaustive `match` over variant, literal, or, range and wildcard patterns with `if` guards, plus `val`-binding destructuring of structs and arrays |
| **Arrays, tuples, collections** | `[T; N]`, tuples, zero-copy slices `&[T]` / `&mut [T]`, and heap-backed `Vec<T>` / `HashMap<K, V>` / `BTreeMap<K, V>` / `String` ([reference](docs/language-reference/types.md)) |
| **Tensors** | `Tensor<T, [d0, ...]>` with shapes checked at compile time: broadcasting, `a @ b` matmul, slicing, shape generics, named and dynamic axes, reductions, sorting, `einsum`, `.map` / `.zip` / `.reduce` ([reference](docs/language-reference/tensors.md)) |
| **Automatic differentiation** | `@grad` on a function or method compiles a reverse-mode derivative of a tensor loss beside it, through branches, loops, calls and function values, at compile time and with no gradient tape; `@grad(wrt: [w, self.head.w])` picks the parameters and receiver fields it differentiates; `loss.backward()` runs it and `w.grad()` / `.zero_grad()` read and clear each parameter's gradient, every one checked against finite differences ([reference](docs/language-reference/autodiff.md)) |
| **Strings** | Immutable fat-pointer `string` with slices, concatenation, codepoint iteration, interpolation `"{x:.2}"` and triple-quoted blocks; growable `String` buffer ([reference](docs/language-reference/strings.md)) |
| **Errors** | `Option<T>` and `Result<T, E>` in the implicit prelude as ordinary generic enums; `??` unwraps with a lazy fallback, `?` propagates, `val-else` exits the scope, `checked_*` arithmetic reports overflow |
| **Ownership** | Move-by-default, `Copy`, borrows with flow-sensitive exclusivity, lifetime elision, deterministic `Drop`, and `pool { }` arena blocks ([reference](docs/language-reference/memory-model.md)) |
| **Modules** | Every `.nr` file is a module, `mod.nr` directories nest, inline `module { }` blocks group; `import` with renames and re-export facades, private-by-default visibility, implicit prelude ([reference](docs/language-reference/modules.md)) |
| **Toolchain** | `neurc check` / `run` / `compile` on inkwell 0.10 and LLVM 20, `--emit obj` for C and NumPy interop, buffered `print` / `println`, and a `panic` / `assert` runtime with located diagnostics |

> **Alpha memory note.** Stack values, literals, the owning collections and reassigned bindings
> are all reclaimed, and so is a heap `string` stored into a struct field, an array or tuple
> element, a call's argument, a call's return value, or a collection slot. What still leaks is the
> handful of storing positions whose owner the compiler cannot prove, each of which holds one
> buffer rather than handing out a dangling one. Full detail, and the reason the analysis answers
> conservatively, is in the [memory model](docs/language-reference/memory-model.md).

---

## Performance

`neurc compile -O 3` hands the module to the same LLVM 20 pipeline `clang -O2` uses. The default
is `-O 0`, checked arithmetic with no optimization, so pass `-O 3` before drawing any conclusion
about speed.

Best of nine runs on one machine, lower is better. Reproduce with `python benchmarks/run.py`,
which builds all three implementations of each program and refuses to report timings if they
disagree on output.

| Benchmark | What it stresses | Neuro `-O 3` | `clang -O2` | Python 3.14 |
|---|---|---|---|---|
| `mandelbrot` | scalar `f64` in a tight loop | 166 ms | 166 ms | 5791 ms |
| `vector_sum` | `Vec` push, indexed sweep | 25 ms | 26 ms | 10068 ms |
| `call_overhead` | recursion, call and inline cost | 45 ms | 51 ms | 1389 ms |
| `print_lines` | integer holes to standard output | 13 ms | 22 ms | 110 ms |
| `format_floats` | `f64` holes at a fixed precision | 118 ms | 109 ms | 214 ms |
| `int_divide` | guarded `/` and `%`, opaque divisor | 96 ms | 89 ms | 1318 ms |

Absolute times belong to the machine, and the Python column to whichever `python3` is on your
PATH. Two rows are worth a word: `print_lines` beats C because an integer hole renders through a
digit loop instead of `snprintf`, and `int_divide` is the one place the compiler spends rather
than saves, since `/` and `%` guard the operand pairs the hardware leaves undefined.

---

## Quick Roadmap

Each numbered phase is a MAJOR-version milestone: completing **Phase N** ships **v(N+1).0.0**.

| Phase | Goal | Status |
|:---:|---|:---:|
| **1** | **Core Language**: types, control flow, LLVM backend, ownership and borrow checking, generics, traits, closures, enums and pattern matching, error handling, modules | Complete |
| **2** | **Tensors and MLIR**: first-class tensor types lowered through MLIR Linalg, the pool allocator, and the value model they need | Complete |
| **3** | **Automatic differentiation**: a reverse-mode source-to-source transform over Neuro's own typed HIR, `@grad(wrt: ...)`, `.backward()` / `.zero_grad()`, higher-order derivatives, elementwise math | In progress |
| **4** | **GPU acceleration**: MLIR GPU dialects (nvgpu / rocdl), `@gpu`, `KernelOut<T>` aliasing model, device memory pool, CPU fallback | Planned |
| **5** | **Neural network standard library**: `TrainableTensor`, `ParameterList`, optimizers, `@model`, Dense / Conv2d / Attention, `.nrm` serialization | Planned |
| **6** | **Async runtime**: `async func`, `Future<T>`, `spawn`, `join` / `race`, an executor for data-loader and I/O overlap | Planned |
| **7** | **Interop**: Python FFI via DLPack, spread operator, advanced pattern matching, custom attributes, `defer` | Planned |
| **8** | **Developer experience**: debug info, incremental compilation, Language Server Protocol, formatter, `@test` runner | Planned |
| **9** | **Distribution**: the `neurpm` package manager, cross-OS installer and self-updater, signed binaries, CPU parallelism, further optimization passes | Planned |

Phase 2 is complete: **2A** standard I/O and spec stragglers, **2B** tensor core, **2C** MLIR
lowering, **2D** the pool allocator, **2E** the value model and **2F** functional sugar — the
`|>` and `>>` operators, Einstein notation, and the `.map` / `.zip` / `.reduce` traversals —
all shipped. Phase 3 now trains: `@grad` on a function or a method emits a reverse-mode
gradient function over tensor arithmetic, `if`, `while`, calls to user functions and function values, `wrt:` picks what
it differentiates down to a model's own fields, `.backward()` runs it and parks each
gradient beside its parameter for `.grad()` to read, and every derivative the compiler
generates is measured against central finite differences of the compiled function.

---

## Architecture

Neuro follows [Vertical Slice Architecture](VSA.md): code is organized by language feature, not by
technical layer.

```
compiler/
├── infrastructure/          # Shared, zero-business-logic crates
│   ├── ast-types/           #   AST node definitions
│   ├── shared-types/        #   Primitives shared across slices
│   └── neuro-hir/           #   Typed High-Level IR (frontend <-> backend contract)
├── lexical-analysis/        # Tokenizer (logos, Unicode XID)
├── syntax-parsing/          # Pratt + statement parser -> AST
├── semantic-analysis/       # Type checker, scope and borrow analysis
├── hir-lowering/            # Type-checked AST -> typed HIR
├── llvm-backend/            # HIR -> object code (inkwell 0.10 / LLVM 20)
├── mlir-backend/            # HIR -> MLIR linalg (off-by-default `mlir` feature)
└── neurc/                   # CLI compiler driver
```

Today a `.nr` file travels: **tokens → AST → type-checked AST → typed HIR → LLVM object code →
system linker**. The tensor path forks after HIR into MLIR (linalg, tensor, func, arith) and will
carry the GPU dialects as Phase 4 lands; the AD transform reads the same typed HIR. Stage by
stage:
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
way to start. Work is most useful in **Phase 3 (Automatic Differentiation)**. The engine is
Neuro's own reverse-mode transform over the typed HIR, where tensor shapes are still in the
types, and every gradient it generates is checked against central finite differences of the
compiled function by `tools/grad_differential.py`.

See also [SECURITY.md](SECURITY.md) and the [Code of Conduct](CODE_OF_CONDUCT.md).

---

## License

[Neuro Shared Source License v2.1](LICENSE). The license covers the compiler, not what you build
with it.

You may freely use, study and modify the compiler for any personal or internal purpose, write
Neuro programs and distribute or sell the compiled output under any terms you choose, build tools
and editor integrations that call into the compiler, and contribute code back. Only redistributing
the compiler itself, or a fork of it, as part of a commercial product requires a commercial
license.

Neuro is pre-stabilization, and the license guards three risks specific to that: commercial
re-packaging before the spec is stable, AI-assisted reproduction of the compiler for a competing
product, and misleading forks that fragment an early ecosystem. A permissive license becomes
possible once the language stabilizes.

## Acknowledgments

Inspired by Rust (ownership, type system), Python (AI ecosystem simplicity), Swift (ergonomics)
and Mojo (AI-first design). Built with [inkwell](https://github.com/TheDan64/inkwell),
[logos](https://github.com/maciejhirsz/logos) and [LLVM](https://llvm.org/).
