# End-to-End Compilation

**Status**: Implemented · AST → typed HIR → LLVM
**Slice**: `compiler/neurc` (orchestrator)
**Dependencies**: `lexical-analysis`, `syntax-parsing`, `module-resolution`, `semantic-analysis`, `hir-lowering`, `llvm-backend`

---

## Overview

The `neurc` compiler driver provides end-to-end compilation from Neuro source files (`.nr`) to
native executables. `neurc` is the only crate permitted to depend on every feature slice; it owns
pipeline orchestration and contains no feature business logic itself (VSA).

## Architecture

### Compilation Pipeline

```
Source File (.nr)
    ↓
┌──────────────────────────────────────────────────────────────┐
│ 1. Read Source (fs::read_to_string)                          │
├──────────────────────────────────────────────────────────────┤
│ 2. Lexical Analysis + Parsing (syntax_parsing::parse)        │
│    - Tokenization (logos)                                     │
│    - AST construction (Pratt + statement parser)             │
├──────────────────────────────────────────────────────────────┤
│ 3. Module Resolution (module_resolution::resolve_program)    │
│    - Expands every module the root file reaches into one     │
│      program: `.nr` files, `mod.nr` directories, inline      │
│      `module { }` blocks, imports and re-exports             │
│    - Verifies qualified paths and item/field visibility,     │
│      then erases the qualifiers into one flat namespace      │
│    - The driver prepends the prelude here, unless the root    │
│      file opted out with `@no_prelude`                       │
├──────────────────────────────────────────────────────────────┤
│ 4. Semantic Analysis (semantic_analysis::type_check)         │
│    - Type checking, scope resolution                         │
│    - Emits warnings (e.g. lints)                             │
├──────────────────────────────────────────────────────────────┤
│ 5. HIR Lowering (hir_lowering::lower_program)                 │
│    - AST → typed High-Level IR (neuro-hir)                   │
│    - Every expression carries its resolved type             │
├──────────────────────────────────────────────────────────────┤
│ 6. Code Generation (llvm_backend::compile)                   │
│    - Consumes the typed HIR directly                         │
│    - LLVM IR generation (inkwell / LLVM 20)                  │
│    - Object code emission                                     │
├──────────────────────────────────────────────────────────────┤
│ 7. Write Object File (tempfile)                              │
│    - Temporary `.o` / `.obj`; removed after linking          │
├──────────────────────────────────────────────────────────────┤
│ 8. Link Executable                                           │
│    - System linker driver invocation + C runtime linking     │
└──────────────────────────────────────────────────────────────┘
    ↓
Native Executable (`.exe` on Windows, no extension on Unix)
```

The typed HIR (`neuro-hir`) is the stable, backend-agnostic contract between the frontend (parser +
type checker) and the backends. `llvm-backend` consumes it today; the experimental `mlir-backend`
consumes the same HIR behind the off-by-default `mlir` feature (1D scaffold). See the
[HIR Lowering](components/hir-lowering.md) and [LLVM Backend](components/llvm-backend.md) component
docs.

## Implementation

### Core Function: `compile_file`

```rust
fn compile_file(input: &Path, output: Option<&Path>, optimization: OptimizationLevelSetting) -> Result<()>
```

**Purpose**: Orchestrates the complete compilation pipeline from source file to executable.

Stages, in order: read source → resolve modules and parse (`module_resolution::resolve_program`,
with the parser and the prelude injected by the driver) → `semantic_analysis::type_check` →
`hir_lowering::lower_program` → `llvm_backend::compile(&hir, optimization, &source, &path)` → write
temporary object file → link.

**Error Handling Strategy**:
- Uses `anyhow::Context` for error-chain construction; each stage adds contextual information.
- Fail-fast: stops at the first error, prints a detailed message to stderr, exits non-zero.
- The type checker reports every type error it finds before the run stops; parse errors stop at
  the first one.

**Example Error Output** (a program with a type mismatch):
```
Type errors found:
  1. cannot apply binary operator + to types string and i32 at Span { start: 29, end: 36 }
Compilation failed: Type checking failed
  Caused by (1): 1 type error(s) found
```

### `check` vs `compile`

`neurc check` runs stages 1 through 5 (read, resolve + parse, type-check, HIR lowering) and stops;
it validates a program (including that it lowers cleanly to HIR) without producing a binary.
`neurc compile` runs the full pipeline.

### Linking

The driver shells out to a linker driver (a C compiler front-ending the real linker, which brings
the C runtime and startup code). On Unix it always invokes `cc`. On Windows it tries, in order:
`clang`, then `lld-link`, then MSVC `cl.exe` (which locates the real `link.exe`).

| Platform | Driver(s) tried | Notes |
|----------|-----------------|-------|
| Windows | `clang` → `lld-link` → `cl.exe` | First one that links wins; requires one of these toolchains |
| Linux | `cc` (gcc or clang) | Requires a C compiler installed |
| macOS | `cc` (clang) | Provided by the Xcode Command Line Tools |

## CLI Integration

```bash
neurc check   <INPUT>            # Stages 1-5: resolve + parse, type-check, lower to HIR
neurc compile <INPUT> [OPTIONS]  # Full pipeline to a native binary
```

**Options** (for `compile`):
- `-o, --output <FILE>`, output executable path (defaults to the input filename, `.exe` on Windows)
- `-O <LEVEL>`, optimization level (0 to 3)

**Examples**:
```bash
neurc check   examples/hello.nr
neurc compile examples/hello.nr
neurc compile examples/hello.nr -o bin/hello
RUST_LOG=debug neurc compile examples/hello.nr   # debug logging
```

### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Compilation succeeded |
| 1 | Compilation failed (syntax, type, HIR-lowering, codegen, or link error) |

## Testing

End-to-end coverage lives in `compiler/neurc/tests/`, e.g. `hir_lowering.rs` exercises the
AST → HIR step, and per-feature suites (`arrays.rs`, `drop_destructors.rs`, `string_concat.rs`,
`string_slice.rs`, …) compile and run real programs, asserting exit codes and output.
`cargo test --workspace` runs the whole suite; the `mlir`-feature tests are additional and
feature-gated.

## Known Limitations

1. **Debug information**: no DWARF/PDB generation yet.
2. **Flat namespace**: module resolution merges every file into one namespace; two modules
   declaring the same top-level name collide rather than nesting.
3. **System toolchain required** for linking (`clang`/MSVC on Windows, `gcc`/`clang` on Unix); no
   bundled linker.

## Future Enhancements

- Debug information (`-g`), position-independent code, cross-compilation, LTO.
- Parallel / incremental compilation and build caching.
- The MLIR tensor/autodiff/GPU path (Phase 2+), lowering the same typed HIR via `mlir-backend`.

## Setup

LLVM 20 with `LLVM_SYS_201_PREFIX` set is required to build the compiler. See the
[Installation Guide](../getting-started/installation.md) for per-platform instructions (Linux,
macOS, Windows) and the optional MLIR backend setup. Common build problems are covered in
[Troubleshooting](../guides/troubleshooting.md).

## References

- [CONTRIBUTING.md](../../CONTRIBUTING.md), development guidelines and architecture rules
- [CHANGELOG.md](../../CHANGELOG.md), version history
- [compiler/neurc/src/main.rs](../../compiler/neurc/src/main.rs), implementation
- [Installation Guide](../getting-started/installation.md), toolchain setup
