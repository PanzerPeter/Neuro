# Neuro Documentation

**Status**: Phase 1 (Core Language) in progress — sub-phases 1A–1E complete, 1F (generics, traits & dispatch) active — Alpha Development

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
- [Control Flow](language-reference/control-flow.md) — if/else, while, loop, range-for, break/continue
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
- [HIR Lowering](compiler/components/hir-lowering.md) — AST → typed High-Level IR (`neuro-hir`)
- [LLVM Backend](compiler/components/llvm-backend.md) — Native code generation (from HIR)
- [MLIR Backend](compiler/components/mlir-backend.md) — Experimental HIR → MLIR path (1D scaffold, off by default)

## What is Neuro?

Neuro is a compiled language designed for high-performance AI workloads. It generates native code via an LLVM 20 backend, with a roadmap toward MLIR-based tensor operations, IR-level automatic differentiation (Enzyme), and GPU acceleration via MLIR GPU dialects.

Key design goals:

- **Static typing** with inference for safety and performance
- **Tensor primitives** as first-class language types (Phase 2+)
- **IR-level AD** via Enzyme MLIR (no runtime gradient tape) (Phase 3+)
- **GPU acceleration** via MLIR `nvgpu`/`rocdl`/Triton dialects (Phase 4+)
- **Zero-copy Python interop** via DLPack (Phase 7+)

## Current Features

### Types

- Primitive integers: `i8`, `i16`, `i32`, `i64`, `u8`, `u16`, `u32`, `u64`
- Floating point: `f32`, `f64`
- Half-precision: `f16`, `bf16` — scalar primitives with a narrow storage/cast/compare contract (no arithmetic; compute in `f32`)
- Boolean: `bool`
- Character: `char` — a single 32-bit Unicode scalar value
- String: fat-pointer ABI (`{ ptr, i64 }`), literals with escape sequences (`\n`, `\t`, `\"`, `\\`, `\xNN`, `\u{NNNN}`)
- Integer and float literal type suffixes: `42i64`, `255u8`, `1.5f32`, `2.0f64`, `1.5f16`, `0.02bf16`
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
- **Generics** (1F): generic functions `func identity<T>(x: T) -> T`, generic
  structs `struct Pair<T, U>`, and generic inherent impls `impl<T> Wrapper<T>`,
  monomorphized (one specialized copy per concrete type-argument set, zero runtime
  cost); type arguments inferred from value/field arguments or written explicitly
  (`Pair<i32, f64>`); trait bounds `<T: Trait>` are enforced; type arguments
  restricted to `Copy` this phase
- **Traits** (1F): `trait` declarations with required and default (provided)
  methods; `impl Trait for Type` checked for conformance; trait-bounded generics
  `func f<T: Shape>(x: &T)` dispatch trait methods on the type parameter, checked at the
  call site. Fully monomorphized and erased — no vtable, zero runtime cost. Associated
  types land later in 1F
- **Static & dynamic dispatch** (1F): `impl Trait` in argument position
  (`func train(m: &impl Model)`) and return position (`func make() -> impl Shape`) is
  anonymous-generic sugar — monomorphized, zero cost; each `impl Trait` parameter is its
  own anonymous type parameter. `dyn Trait` is a runtime trait object behind
  `&dyn Trait` / `&mut dyn Trait`, dispatched through a per-(trait, type) vtable, so one
  function body serves every implementor. Object safety is enforced: every method of a
  `dyn`-usable trait must take `&self` or `&mut self`

### Control Flow

- `if` / `else if` / `else` as statements and as **expressions** (value-producing)
- Bare block expressions as values — statements newline-separated, the final expression is the block's value:
  ```neuro
  val r = {
      val a = 3
      val b = 4
      a + b
  }
  ```
- `while` loops
- `loop { }` infinite loops (canonical infinite loop; exit via `break`)
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
- Null-coalescing `??`: tokenized and parsed (R-to-L associativity); codegen deferred to 1G
- String equality: `==` and `!=` via length-check + `memcmp`
- Builtin method dispatch on primitive & string receivers: `string.len() -> u64` (O(1) fat-pointer read), `.clone()`, and `.slice(a..b) -> &string` (zero-copy sub-slice; panics on out-of-bounds or mid-codepoint boundary)

### Structs and Methods (1E)

- `struct` definitions with any primitive or struct field types
- `impl` blocks: instance methods (`&self` and `&mut self`) and associated functions (`TypeName::func`)
- `&mut self` methods mutate `self.field` in place (passed by pointer); calling one needs a `mut` receiver and takes an exclusive borrow for the call. Consuming `self` is still rejected
- Nominal typing; forward-reference support (definition order independent)

### Arrays (1E)

- Fixed-size `[T; N]` of `Copy` scalar elements: literals (with element-type inference), index read/write, `.len()` (compile-time `u64`)
- Iteration `for x in arr` and `for x in &arr`, lowered as a counted loop over the storage
- Out-of-bounds index panics in debug builds (`-O0`); release builds omit the check

### Tuples (1E)

- Anonymous `(T1, T2, ...)` of `Copy` elements: literals, `.0`/`.1` constant index access
- Destructuring binds `val (a, b) = t` with `_` wildcards and nesting (`val ((a, b), c) = ...`)
- Usable as function parameters and return types; a single `(x)` stays grouping

### Struct + array destructuring (1E)

- Struct patterns `val Point { x, y } = p` bind each field by its own name
- Array patterns `val [a, b, c] = arr` bind positionally; `val [first, ..rest] = arr`
  captures the remainder as a fresh `[T; N - k]` array, and a bare `..` ignores it
- Rest-less array patterns are arity-checked against the array length; patterns nest
  and work with `mut`

### Enums (1E)

- Tagged unions `enum E { A, B(i32), C { x: f64 } }` with unit, tuple, and struct-field variants
- Construct via `E::A` / `E::B(1)` / `E::C { x: 1.0 }`; usable as bindings, function
  parameters/returns, and struct fields; an enum is `Copy`
- Scalar `Copy` payloads only; non-generic

### Pattern Matching (1E)

- `match` as an exhaustive expression; the first matching (and guard-passing) arm supplies the value
- Patterns: `_` wildcard, bare binding, literals, `a..=b` / `a..b` ranges, `|` or-patterns, and enum
  variant patterns (`E::Unit`, `E::Tuple(a)`, `E::Struct { field }`) that bind their payload
- `if` guards on arms; exhaustiveness enforced (enum variant coverage / both bools / a `_` arm)
- Phase-1E limits: scrutinee is enum/integer/`char`/`bool`; payload sub-patterns are binding-or-`_`;
  `|`-alternatives may not bind

### Newtypes (1E)

- `newtype Meters = i32` creates a distinct nominal type wrapping an inner type
- Not interchangeable with the inner type (unlike a transparent `type` alias)
- Construct with `Meters(30)`; read the wrapped value with `.0`; forwards `Copy`/`Clone`
- Usable as a binding, function parameter/return, and struct field
- Phase-1E limit: the inner type must be `Copy`; operator/trait impls on a newtype await 1F+

### Compilation

- Full LLVM 20 backend via inkwell 0.9.0
- Native executable generation
- Signedness-aware integer codegen
- 929 tests passing across all components

## Compilation Pipeline

```
Source File (.nr)
  → Lexical Analysis   — tokenization
  → Syntax Parsing     — AST generation
  → Semantic Analysis  — type checking
  → HIR Lowering       — AST → typed High-Level IR (neuro-hir)
  → LLVM Backend       — object code (consumes HIR; inkwell / LLVM 20)
  → System Linker      — native executable
```

The typed **High-Level IR** (`neuro-hir`) is the backend-agnostic contract: every backend lowers
from it. The LLVM backend consumes it today; the experimental `mlir-backend` consumes the same HIR
behind the off-by-default `mlir` feature (1D scaffold).

**Planned extension (Phase 2+):**
```
Tensor/AI path (lowers the same typed HIR):
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

See the [Quick Roadmap in the project README](../README.md#quick-roadmap) for the phase-by-phase status, and [CONTRIBUTING.md](../CONTRIBUTING.md#current-contribution-priorities) for the active Phase 1 (sub-phase 1E) priorities.

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
| CPU codegen | inkwell | 0.9.0 (LLVM 20) |
| MLIR construction | melior | 0.25.1 (LLVM/MLIR 20) — integrated 1D in the `mlir-backend` slice behind the off-by-default `mlir` feature |
| Autodiff (Phase 3+) | Enzyme (MLIR dialect) | built against LLVM 20 |
| GPU (Phase 4+) | MLIR nvgpu/rocdl/Triton | LLVM 20 backends |

## Project Resources

- [README.md](../README.md) — project overview
- [CHANGELOG.md](../CHANGELOG.md) — version history
- [CONTRIBUTING.md](../CONTRIBUTING.md) — contribution guidelines and architecture rules
- [LICENSE](../LICENSE) — Neuro Shared Source License v2.1

---

**Last Updated**: 2026-06-28
**Version**: Phase 1 (Core Language) in progress — 1A–1D complete, 1E active (v1.53.0)
**Rust**: 1.85+ | **LLVM**: 20 | **inkwell**: 0.9.0
