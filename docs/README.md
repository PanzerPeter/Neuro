# Neuro Documentation

**Status**: Alpha. Phase 1 (Core Language) is in progress. Sub-phases 1A through 1G are complete, with `Option` / `Result`, the standard collections, `val-else`, the `?` propagation operator, error-path outlining, multi-file compilation, `import`, `export` visibility, inline modules with re-exports, and the implicit prelude all landed. Sub-phase 1H (language cleanup) is next.

## Quick Links

- [Installation Guide](getting-started/installation.md)
- [Quick Start](getting-started/quick-start.md)
- [Language Reference](language-reference/types.md)
- [Troubleshooting](guides/troubleshooting.md)

## Documentation Structure

### Getting Started

- [Installation Guide](getting-started/installation.md): Install Neuro on Linux or macOS
- [Quick Start Guide](getting-started/quick-start.md): Basic usage and workflow
- [Your First Program](getting-started/first-program.md): Step-by-step tutorial

### Language Reference

- [Types](language-reference/types.md): Primitive types (integers, floats, bool, string)
- [Variables](language-reference/variables.md): `val`, `mut`, reassignment, scoping
- [Functions](language-reference/functions.md): Declarations, parameters, implicit returns
- [Expressions](language-reference/expressions.md): Expression syntax and evaluation
- [Control Flow](language-reference/control-flow.md): if/else, while, loop, range-for, break/continue
- [Operators](language-reference/operators.md): Arithmetic, comparison, logical, bitwise, cast operators
- [Structs](language-reference/structs.md): User-defined types, methods, associated functions
- [Modules](language-reference/modules.md): Multi-file programs, `mod.nr` directories, qualified paths, `import`, inline `module` blocks, `export import` re-exports, `export` visibility

### User Guides

- [CLI Usage](guides/cli-usage.md): `neurc check`, `neurc compile`, flags
- [Troubleshooting](guides/troubleshooting.md): Common problems and solutions

### Compiler Architecture

- [Compilation Pipeline](compiler/compilation.md): End-to-end compilation process
- [Lexical Analysis](compiler/components/lexical-analysis.md): Tokenizer
- [Syntax Parsing](compiler/components/syntax-parsing.md): AST generation
- [Semantic Analysis](compiler/components/semantic-analysis.md): Type checking
- [HIR Lowering](compiler/components/hir-lowering.md): AST → typed High-Level IR (`neuro-hir`)
- [LLVM Backend](compiler/components/llvm-backend.md): Native code generation (from HIR)
- [MLIR Backend](compiler/components/mlir-backend.md): Experimental HIR → MLIR path (1D scaffold, off by default)

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
- Half-precision: `f16` and `bf16`, scalar primitives with a narrow storage/cast/compare contract (no arithmetic; compute in `f32`)
- Boolean: `bool`
- Character: `char`, a single 32-bit Unicode scalar value
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
  call site. Fully monomorphized and erased, so there is no vtable and no runtime cost. Associated
  types land later in 1F
- **Static & dynamic dispatch** (1F): `impl Trait` in argument position
  (`func train(m: &impl Model)`) and return position (`func make() -> impl Shape`) is
  anonymous-generic sugar: monomorphized at zero cost, and each `impl Trait` parameter is its
  own anonymous type parameter. `dyn Trait` is a runtime trait object behind
  `&dyn Trait` / `&mut dyn Trait`, dispatched through a per-(trait, type) vtable, so one
  function body serves every implementor. Object safety is enforced: every method of a
  `dyn`-usable trait must take `&self` or `&mut self`
- **Closures & lambdas** (1F): anonymous callables `|x: i32| x * x`,
  `|x: i32| -> i32 { ... }`, `|| expr`, and `move |x| ...`, capturing Copy free variables
  by value; the function type `(T1, ...) -> R`; and higher-order functions
  (`func apply(v: i32, f: (i32) -> i32)`). Each closure compiles to a
  `{ fn_ptr, env_ptr }` value with no heap allocation. Parameter-type inference and passing
  a closure to a *generic* higher-order function come later

### Control Flow

- `if` / `else if` / `else` as statements and as **expressions** (value-producing)
- Bare block expressions as values, with statements newline-separated and the final expression is the block's value:
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
- Null-coalescing `??`: unwraps an `Option<T>` / `Result<T, E>` to its payload, else evaluates the fallback (R-to-L associativity, lazy fallback, `Err` payload discarded)
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
- Scalar `Copy` payloads only

### Generic enums, `Option` and `Result` (1G)

- `enum Slot<T> { Filled(T), Vacant }` is monomorphized per type-argument set, so each instance
  is its own tagged union with its own payload width and zero runtime cost
- Type arguments come from the expected type, the payload (`Slot::Filled(4)` → `T = i32`), or the
  enclosing function's return type (which is what a tail `if` branch relies on)
- A `match` pattern names the base enum and binds payloads at the scrutinee instance's types
- `Option<T> { Some(T), None }` and `Result<T, E> { Ok(T), Err(E) }` come from the implicit prelude:
  available in every program with no declaration and no import, the four variants included, so
  `Some(n)` and `Err(e)` read bare. A local declaration of the same name shadows them
- `checked_add` / `checked_sub` / `checked_mul` on any integer type return `Option<T>` over the
  receiver's type: `Option::Some(result)` when it fits, `Option::None` on overflow. It is branchless:
  the LLVM `*.with.overflow` overflow bit picks the variant
- `??` reads either type without a `match`: `lookup(k) ?? 0` yields the `Some`/`Ok` payload, else
  the fallback. The `Err` payload is discarded, the fallback is lazy, and `a ?? b ?? c` chains
  right-to-left. Desugared to a two-arm `match` during HIR lowering, so neither backend sees it
- `expr?` propagates instead of defaulting: it yields the `Some`/`Ok` payload, or returns the
  failure (`None` / `Err(e)`, rebuilt as the enclosing function's own instance) to the caller. The
  function must return the same fallible enum, and the error travels unconverted, so there is no
  `From`/`Into`. Also a lowering-time desugar to a two-arm `match` whose failure arm `return`s
- `val PATTERN = value else |binding| { ... }` unwraps a variant or leaves the scope: the pattern's
  bindings stay live for the rest of the enclosing block, and the `else` branch must diverge
  (`return` / `break` / `continue` / `panic` / `unreachable`). The `else |name|` form is
  type-directed: a `Result`'s `Err` payload, nothing on an `Option` (only `|_|`), and the whole
  scrutinee for any other enum
- Limits: scalar `Copy` payloads per instance (`Option<string>` awaits heap payloads), `Copy` type
  arguments, no `impl` blocks on enums, no lifetime parameters

### Standard Collections (1G)

- `Vec<T>`, `HashMap<K, V>`, `BTreeMap<K, V>` are heap-backed library types the compiler knows by
  name, since the language exposes no allocator to build them from
- Not `Copy`: they move on assignment and free their buffer at scope exit; a mutating method needs
  a `mut` binding
- `Vec`: `push` / `pop` / `get` / `len` / `clear`, `v[i]` read+write (bounds-checked in every
  build), and `for x in v`
- Maps: `insert` / `get` / `contains_key` / `remove` / `len` / `clear` / `keys`; `keys()` returns a
  `Vec<K>`, ascending for `BTreeMap`
- Keys are integer / `bool` / `char` / `string`, or a struct with `impl PartialEq` plus
  `impl Hashable` (hashed) or `impl Comparable` (ordered). Raw float keys are rejected, because the
  prelude's `OrderedF32` / `OrderedF64` wrappers reject NaN and provide the total order
- `Hashable` is a compiler-known lang-item trait: `func hash(&self) -> u64`
- Limits: `pop` / `get` build an `Option<T>`, so they need an `Option`-carryable element type;
  a `string` inside a collection is not freed with it

### Modules & Visibility (1G)

- A program may span several files: every `.nr` file is a module, and a directory holding a
  `mod.nr` is a module with children. You compile the root; every module it reaches comes with it
- An **`import`, or a qualified path** written without one, is what pulls a module in:
  `math::sqrt`, `utils::io::read`, `geometry::Point`, in value position and in type annotations
  alike
- `import math`, `import ./utils::io`, `import math::{sqrt, sin}`, `import math::sin as sine`,
  `import math::matrix as mat`, and `import Shape::{Circle, Square}` are all available. An imported
  variant reads unqualified as a value and as a `match` pattern
- Only referenced modules load, so a directory of unrelated single-file programs still compiles one
  at a time. A leaf `math.nr` has no children; only a `mod.nr` directory opens a level
- A locally declared type wins over a same-named file, so `Point::new` keeps meaning the associated
  function even with a `Point.nr` beside it
- A declaration is **private to its file** unless it carries `export`, and a struct field carries
  its own marker, so an exported struct may still keep a field to itself. From another module a
  private field can be neither read, written, listed in a literal, destructured, nor copied through
  `..base`. Methods have no marker: an `impl` declares no name, so its methods follow their type
- Item visibility is reported while modules resolve; field visibility needs the receiver's type and
  is reported by the type checker. Neither rule is visible in a single-file program, since one file is
  one module
- An inline **`module Name { ... }` block** is a module with no file of its own: same visibility
  rule, reached by the same qualified path, and blocks nest. The file declaring a block is outside
  it, so `export` is the only way in; a block has no file children, and it wins over a same-named
  file beside it
- **`export import`** re-exports: it binds names locally like any import *and* makes them reachable
  through the importing module, so a facade offers a flatter API than its internals. A rename is
  undone on the way through, and facades chain. Only an item can be re-exported. A module or an
  enum variant is an error rather than a silent no-op
- Modules still share one flat namespace, so qualification is checked but never required, and a
  name declared by two loaded modules is a reported collision rather than a silent winner, even
  when both keep it private, and a block buys a private surface rather than a private namespace
- Every module begins with an **implicit prelude**: `Option`, `Result`, their variants `Some` /
  `None` / `Ok` / `Err`, and `println` / `print` are in scope with no `import` of any kind. A local
  declaration of one of those names, or an explicit import of it, wins inside that module rather
  than colliding. A file opts out with **`@no_prelude`** on its first line. On a non-root file that
  drops its bindings; on the root it drops the prelude's declarations from the whole program, since
  the merged namespace is flat
- See the [modules reference](language-reference/modules.md) for the resolution rules and
  diagnostics

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
- Error-path outlining: every panic-family failure path (`panic`, `assert`, `unreachable`, and the
  array, `Vec`, string-slice, and UTF-8-boundary guards) is emitted into a module-private cold
  function and called from the failure site, so the diagnostic machinery never sits inline in the
  function that can fail; guard branches carry `!prof` weights keeping the failure edge off the
  fall-through path
- Full workspace test suite green on every push (see the CI badge in the root [README](../README.md))

## Compilation Pipeline

```
Source File (.nr)
  → Lexical Analysis   : tokenization
  → Syntax Parsing     : AST generation
  → Semantic Analysis  : type checking
  → HIR Lowering       : AST → typed High-Level IR (neuro-hir)
  → LLVM Backend       : object code (consumes HIR; inkwell / LLVM 20)
  → System Linker      : native executable
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

See the [Quick Roadmap in the project README](../README.md#quick-roadmap) for the phase-by-phase status, and [CONTRIBUTING.md](../CONTRIBUTING.md#current-contribution-priorities) for the active Phase 1 (sub-phase 1G) priorities.

## Architecture

Neuro uses Vertical Slice Architecture (VSA): the code is organized by language capability, not by technical layer.

Principles:
1. Slice independence: each feature crate is self-contained
2. Infrastructure sharing: common utilities live in the `infrastructure/` layer and hold no business logic
3. Clear boundaries: `pub(crate)` by default, with `pub` only for slice entry points
4. No cross-slice imports: feature slices do not import from each other

See [CONTRIBUTING.md](../CONTRIBUTING.md) for the full architecture guide.

## Backend Stack

| Component | Library | Version |
|---|---|---|
| CPU codegen | inkwell | 0.9.0 (LLVM 20) |
| MLIR construction | melior | 0.25.1 (LLVM/MLIR 20), integrated 1D in the `mlir-backend` slice behind the off-by-default `mlir` feature |
| Autodiff (Phase 3+) | Enzyme (MLIR dialect) | built against LLVM 20 |
| GPU (Phase 4+) | MLIR nvgpu/rocdl/Triton | LLVM 20 backends |

## Project Resources

- [README.md](../README.md): project overview
- [CHANGELOG.md](../CHANGELOG.md): version history
- [CONTRIBUTING.md](../CONTRIBUTING.md): contribution guidelines and architecture rules
- [LICENSE](../LICENSE): Neuro Shared Source License v2.1

---

**Last Updated**: 2026-07-27
**Version**: Phase 1 (Core Language) in progress. Sub-phases 1A through 1G are complete; 1H is next.
**Rust**: 1.85+ | **LLVM**: 20 | **inkwell**: 0.9.0
