# Neuro Documentation

**Status**: Alpha. Phases 1 (Core Language) and 2 (Tensors and MLIR) are complete; Phase 3
(Automatic Differentiation) is open.
Per-phase status lives in one place, the [Quick Roadmap](../README.md#quick-roadmap); what each
release changed is in [CHANGELOG.md](../CHANGELOG.md).

This file is an index. It describes where things are, not what they do: every feature is defined
once, in the page that owns it.

## What is Neuro?

A compiled language for high-performance AI workloads. It generates native code through an LLVM 20
backend, with a roadmap toward MLIR-based tensor operations, compile-time automatic
differentiation, and GPU acceleration via MLIR GPU dialects.

Design goals:

- **Static typing** with inference, for safety and performance
- **Tensor primitives** as first-class language types (Phase 2+)
- **Compile-time AD** as a source-to-source transform over typed HIR, no runtime gradient tape (Phase 3+)
- **GPU acceleration** via MLIR `nvgpu` / `rocdl` / Triton dialects (Phase 4+)
- **Zero-copy Python interop** via DLPack (Phase 7+)

## Getting Started

- [Installation Guide](getting-started/installation.md): install Neuro on Linux, macOS or Windows
- [Quick Start](getting-started/quick-start.md): basic usage and workflow
- [Your First Program](getting-started/first-program.md): step-by-step tutorial
- [examples/](../examples/): runnable programs, each pinned to its exact exit code and output

## Language Reference

| Page | Covers |
|---|---|
| [Types](language-reference/types.md) | Primitives, literals and suffixes, casts, arrays, slices, tuples, newtypes, aliases, borrows, type inference |
| [Strings](language-reference/strings.md) | The `string` slice type, the growable `String` buffer, interpolation, triple-quoted literals, codepoint iteration |
| [Tensors](language-reference/tensors.md) | `Tensor<T, [dims]>`, construction, element-wise arithmetic and broadcasting, matrix multiplication, indexing and slicing, shape generics, named dimensions, reshaping, reductions, sorting, Einstein notation, functional traversals, dynamic shapes, devices |
| [Automatic Differentiation](language-reference/autodiff.md) | `@grad` on functions and methods, the derivative it generates, `.backward()` / `.grad()` / `.zero_grad()` and the borrows they end, its signature rules, which constructs a differentiated body may use, and how the derivative follows branches, loops and calls |
| [Variables](language-reference/variables.md) | `val`, `mut`, reassignment, scoping |
| [Functions](language-reference/functions.md) | Declarations, parameters, named arguments, returns, generics, dispatch, closures |
| [Expressions](language-reference/expressions.md) | Expression syntax and evaluation order |
| [Control Flow](language-reference/control-flow.md) | `if`/`else`, `while`, `loop`, `for`, `break`/`continue`, `match`, `val-else`, `pool` arena blocks |
| [Operators](language-reference/operators.md) | Arithmetic, comparison, logical, bitwise, cast, overloading, assignment places, `??`, `?`, `\|>`, `>>` |
| [Structs](language-reference/structs.md) | User-defined types, `impl` methods, associated functions, derives, enums |
| [Memory Model](language-reference/memory-model.md) | What is reclaimed and when: stack values, collections, reassigned bindings, `pool` blocks, and what still leaks in alpha |
| [Modules](language-reference/modules.md) | Multi-file programs, `mod.nr` directories, qualified paths, `import`, inline `module` blocks, `export import` re-exports, visibility |

## User Guides

- [CLI Usage](guides/cli-usage.md): `neurc check`, `neurc run`, `neurc compile`, flags, `--emit obj` and the NumPy interop recipe
- [Editor Support](guides/editor-support.md): syntax highlighting for `.nr` files, and keeping the grammar in step with the lexer
- [Troubleshooting](guides/troubleshooting.md): common problems and solutions
- [Known Bugs](BUGS.md): the open defect register

## Compiler Architecture

- [Compilation Pipeline](compiler/compilation.md): end-to-end, source file to native binary
- [Lexical Analysis](compiler/components/lexical-analysis.md): tokenizer
- [Syntax Parsing](compiler/components/syntax-parsing.md): AST generation
- [Module Resolution](compiler/components/module-resolution.md): multi-file expansion, imports, visibility
- [Argument Binding](compiler/components/argument-binding.md): named arguments resolved to declaration order
- [Semantic Analysis](compiler/components/semantic-analysis.md): type checking
- [HIR Lowering](compiler/components/hir-lowering.md): AST to typed High-Level IR (`neuro-hir`)
- [LLVM Backend](compiler/components/llvm-backend.md): native code generation from HIR
- [MLIR Backend](compiler/components/mlir-backend.md): experimental HIR to MLIR linalg path, off by default

The typed High-Level IR (`neuro-hir`) is the backend-agnostic contract: every backend lowers from
it. Each slice also keeps a `CONTEXT.md` beside its source, which is the authority on that slice's
current entry points when this directory disagrees.

### Backend stack

| Component | Library | Status |
|---|---|---|
| CPU codegen | inkwell (LLVM 20) | In use |
| MLIR construction | melior (LLVM/MLIR 20) | Tensor arithmetic to linalg, broadcasting included, bufferized through to LLVM IR, behind the off-by-default `mlir` feature |
| Autodiff | Neuro's own reverse-mode HIR transform | Generates `@grad` derivatives, checked against finite differences |
| GPU | MLIR nvgpu / rocdl / Triton | Phase 4+ |

Exact dependency versions live in the workspace `Cargo.toml` files, not here.

## File Extensions

| Extension | Purpose |
|---|---|
| `.nr` | Neuro source files |
| `.nrl` | Compiled library modules (planned) |
| `.nrm` | Serialized model data (planned) |
| `.nrp` | Package definitions (planned) |

## Project Resources

- [README.md](../README.md): project overview and roadmap
- [CHANGELOG.md](../CHANGELOG.md): version history
- [CONTRIBUTING.md](../CONTRIBUTING.md): contribution guidelines and the full architecture rules
- [VSA.md](../VSA.md): the Vertical Slice Architecture ruleset the compiler is built to
- [LICENSE](../LICENSE): Neuro Shared Source License v2.1
