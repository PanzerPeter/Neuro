# Neuro Documentation

**Status**: Phase 1 & Phase 1.5 Complete, Phase 1.7 (ownership) active · Phase 2 overlapping — Alpha Development

## Quick Links

- [Installation Guide](getting-started/installation.md)
- [Quick Start](getting-started/quick-start.md)
- [Language Reference](language-reference/types.md)
- [Troubleshooting](guides/troubleshooting.md)

## Documentation Structure

### Getting Started

- [Installation Guide](getting-started/installation.md) — Install Neuro on Linux or macOS
- [Quick Start Guide](getting-started/quick-start.md) — Basic usage and workflow
- [Your First Program](getting-started/first-program.md) — Step-by-step tutorial

### Language Reference

- [Types](language-reference/types.md) — Primitive types (integers, floats, bool, string)
- [Variables](language-reference/variables.md) — `val`, `mut`, reassignment, scoping
- [Functions](language-reference/functions.md) — Declarations, parameters, implicit returns
- [Expressions](language-reference/expressions.md) — Expression syntax and evaluation
- [Control Flow](language-reference/control-flow.md) — if/else, while, range-for, break/continue
- [Operators](language-reference/operators.md) — Arithmetic, comparison, logical, bitwise, cast operators
- [Structs](language-reference/structs.md) — User-defined types, methods, associated functions

### User Guides

- [CLI Usage](guides/cli-usage.md) — `neurc check`, `neurc compile`, flags
- [Troubleshooting](guides/troubleshooting.md) — Common problems and solutions

### Compiler Architecture

- [Compilation Pipeline](compiler/compilation.md) — End-to-end compilation process
- [Lexical Analysis](compiler/components/lexical-analysis.md) — Tokenizer
- [Syntax Parsing](compiler/components/syntax-parsing.md) — AST generation
- [Semantic Analysis](compiler/components/semantic-analysis.md) — Type checking
- [LLVM Backend](compiler/components/llvm-backend.md) — Native code generation

## What is Neuro?

Neuro is a compiled language designed for high-performance AI workloads. It generates native code via an LLVM 20 backend, with a roadmap toward MLIR-based tensor operations, IR-level automatic differentiation (Enzyme), and GPU acceleration via MLIR GPU dialects.

Key design goals:

- **Static typing** with inference for safety and performance
- **Tensor primitives** as first-class language types (Phase 3+)
- **IR-level AD** via Enzyme MLIR (no runtime gradient tape) (Phase 4+)
- **GPU acceleration** via MLIR `nvgpu`/`rocdl`/Triton dialects (Phase 5+)
- **Zero-copy Python interop** via DLPack (Phase 6+)

## Current Features

### Types

- Primitive integers: `i8`, `i16`, `i32`, `i64`, `u8`, `u16`, `u32`, `u64`
- Floating point: `f32`, `f64`
- Boolean: `bool`
- String: fat-pointer ABI (`{ ptr, i64 }`), literals with escape sequences (`\n`, `\t`, `\"`, `\\`, `\xNN`, `\u{NNNN}`)
- Integer and float literal type suffixes: `42i64`, `255u8`, `1.5f32`, `2.0f64`
- Contextual numeric literal inference with range validation
- Struct types: definition, instantiation, field access, field mutation

### Variables

- Immutable (`val`) and mutable (`mut`) bindings with type-safe reassignment
- Compile-time constants: `const NAME: Type = expr` at module and function scope
- Lexical scoping

### Functions

- Typed parameters and return types
- Explicit `return` and implicit trailing-expression returns
- Recursion and forward references

### Control Flow

- `if` / `else if` / `else` as statements and as **expressions** (value-producing)
- Bare block expressions as values: `val r = { val a = 3; val b = 4; a + b }`
- `while` loops
- Range-for loops: exclusive (`for i in 0..n`) and inclusive (`for i in 0..=n`)
- `break` and `continue`
- Attribute system: `@allow(prefer_loop_over_while_true)` suppresses the `while true` lint

### Operators

- Arithmetic: `+`, `-`, `*`, `/`, `%`
- Compound assignment: `+=`, `-=`, `*=`, `/=`, `%=`
- Comparison: `==`, `!=`, `<`, `>`, `<=`, `>=` (IEEE-754 ordered for floats)
- Logical: `&&`, `||`, `!`
- Bitwise: `&`, `|`, `^`, `~`, `<<` (integer types only)
- Type cast: `n as f64`, `pi as i32`
- Null-coalescing `??`: tokenized and parsed (R-to-L associativity); codegen deferred to Phase 2
- String equality: `==` and `!=` via length-check + `memcmp`
- Builtin method dispatch on primitive & string receivers; first intrinsic `string.len() -> u64` (O(1) fat-pointer read)

### Structs and Methods (Phase 2)

- `struct` definitions with any primitive or struct field types
- `impl` blocks: instance methods (`&self`) and associated functions (`TypeName::func`)
- Nominal typing; forward-reference support (definition order independent)

### Compilation

- Full LLVM 20 backend via inkwell 0.8.0
- Native executable generation
- Signedness-aware integer codegen
- 492 tests passing across all components

## Compilation Pipeline

```
Source File (.nr)
  → Lexical Analysis   — tokenization
  → Syntax Parsing     — AST generation
  → Semantic Analysis  — type checking
  → LLVM Backend       — object code (inkwell / LLVM 20)
  → System Linker      — native executable
```

**Planned extension (Phase 3+):**
```
Tensor/AI path:
  → Neuro High-Level IR
  → MLIR (linalg / tensor / func / arith)
  → Enzyme MLIR AD pass (@grad)
  → GPU dialects (nvgpu / rocdl / Triton)  or  llvm dialect
  → inkwell → native code
```

## Example Programs

### Hello World (returns 0)

```neuro
func main() -> i32 {
    return 0
}
```

### Factorial (recursive, implicit return)

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

### Range-For Loop

```neuro
func sum_to(n: i32) -> i32 {
    mut total: i32 = 0
    for i in 0..n {
        total += i
    }
    total
}
```

### Neuron (structs + methods + if-expression)

```neuro
struct Neuron {
    weight: f64,
    bias:   f64
}

impl Neuron {
    func new(weight: f64, bias: f64) -> Neuron {
        Neuron { weight: weight, bias: bias }
    }

    func activate(&self, input: f64) -> f64 {
        val z = (input * self.weight) + self.bias
        if z > 0.0 { z } else { 0.0 }  // ReLU
    }
}

func main() -> i32 {
    val n = Neuron::new(0.5, -0.1)
    val out = n.activate(1.0)   // 0.4
    return 0
}
```

More examples in [examples/](../examples/).

## Building from Source

```bash
# Arch Linux / CachyOS
sudo pacman -S llvm20
export LLVM_SYS_201_PREFIX=/usr/lib/llvm20

git clone https://github.com/PanzerPeter/Neuro.git
cd Neuro
cargo build --release
cargo test --workspace
```

See [Installation Guide](getting-started/installation.md) for other distributions.

## Roadmap

See the [Quick Roadmap in the project README](../README.md#quick-roadmap) for the phase-by-phase status, and [CONTRIBUTING.md](../CONTRIBUTING.md#current-contribution-priorities) for the active Phase 1.7 checklist.

## Architecture

Neuro uses **Vertical Slice Architecture (VSA)** — organized by language capabilities, not technical layers.

Principles:
1. **Slice Independence** — each feature crate is self-contained
2. **Infrastructure Sharing** — common utilities in the `infrastructure/` layer (no business logic)
3. **Clear Boundaries** — `pub(crate)` by default; `pub` only for slice entry points
4. **No Cross-Slice Imports** — feature slices do not import from each other

See [CONTRIBUTING.md](../CONTRIBUTING.md) for the full architecture guide.

## Backend Stack

| Component | Library | Version |
|---|---|---|
| CPU codegen | inkwell | 0.8.0 (LLVM 20) |
| MLIR construction (Phase 3+) | melior | LLVM/MLIR 20 |
| Autodiff (Phase 4+) | Enzyme (MLIR dialect) | built against LLVM 20 |
| GPU (Phase 5+) | MLIR nvgpu/rocdl/Triton | LLVM 20 backends |

## Project Resources

- [README.md](../README.md) — project overview
- [CHANGELOG.md](../CHANGELOG.md) — version history
- [CONTRIBUTING.md](../CONTRIBUTING.md) — contribution guidelines and architecture rules
- [LICENSE](../LICENSE) — GPL v3.0 with Neuro Exceptions

---

**Last Updated**: 2026-06-03
**Version**: Phase 1.7 (ownership) active · Phase 2 overlapping (v1.24.1)
**Rust**: 1.85+ | **LLVM**: 20 | **inkwell**: 0.8.0
