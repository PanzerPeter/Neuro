# End-to-End Compilation

**Status**: Implemented · AST → typed HIR → LLVM
**Slice**: `compiler/neurc` (orchestrator)
**Dependencies**: `lexical-analysis`, `syntax-parsing`, `module-resolution`, `argument-binding`, `semantic-analysis`, `hir-lowering`, `llvm-backend`

---

## Overview

The `neurc` compiler driver provides end-to-end compilation from Neuro source files (`.nr`) to
native executables. `neurc` is the only crate permitted to depend on every feature slice; it owns
pipeline orchestration and contains no feature business logic itself (VSA).

## Architecture

### Compilation Pipeline

```text
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
│ 3b. Argument Binding (argument_binding::bind_arguments)      │
│    - Reorders named arguments into declaration order, once   │
│      every module is merged and every callee is visible      │
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
│    - LLVM IR generation (inkwell / LLVM 22)                  │
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
consumes the same HIR behind the off-by-default `mlir` feature, and can carry its scaffold module
on through the `llvm` dialect into a verified inkwell LLVM module. That path is not reachable from
`neurc`: it runs from the slice's own tests. See the [HIR Lowering](components/hir-lowering.md),
[LLVM Backend](components/llvm-backend.md) and [MLIR Backend](components/mlir-backend.md) component
docs.

## Implementation

### Core Function: `compile_file`

```rust
fn compile_file(input: &Path, output: Option<&Path>, optimization: u8, emit: EmitKind) -> Result<PathBuf>
```

**Purpose**: Orchestrates the complete compilation pipeline from source file to artifact, and
returns the path it wrote.

Stages, in order: read source → resolve modules and parse (`module_resolution::resolve_program`,
with the parser and the prelude injected by the driver) → `argument_binding::bind_arguments` →
`semantic_analysis::type_check` → `hir_lowering::lower_program` → check that `main` exists
(unless `--emit` asks for an object or IR) → `llvm_backend::compile` → write the object file
into a temporary directory → link. The rationale for this order lives in the
[`neurc` CONTEXT.md](../../compiler/neurc/CONTEXT.md).

**Error Handling Strategy**:
- Uses `anyhow::Context` for error-chain construction; each stage adds contextual information.
- Fail-fast: stops at the first error, prints a detailed message to stderr, exits non-zero.
- The type checker reports every type error it finds before the run stops; parse errors stop at
  the first one.

**Example Error Output** (a program with a type mismatch):
```text
Type errors found in "concat.nr":
error: cannot apply binary operator + to types string and i32
 --> concat.nr:2:13
  |
2 |     val s = "count: " + 1
  |             ^^^^^^^^^^^^^

Compilation failed: Type checking failed
  Caused by (1): 1 type error(s) found
```

### `check` vs `compile`

`neurc check` runs stages 1 through 5 (read, resolve + parse, bind arguments, type-check, HIR
lowering) and stops; it validates a program (including that it lowers cleanly to HIR) without
producing a binary. `neurc compile` runs the full pipeline. `neurc run` calls `compile_file`
with an output path inside a temporary directory, runs the result, and exits with the
program's own status.

`--emit obj` stops one step short of the linker and writes the object file to the output path
instead. It carries no `main` requirement, because an object may be a library; the default
`--emit exe` still demands an entry point. See
[CLI Usage](../guides/cli-usage.md#emitting-an-object-file) for linking one into a shared
library a foreign DLPack consumer can call.

`--emit llvm-ir` stops one step earlier still and writes the textual LLVM module, with the
host data layout and triple and after `-O`'s pass pipeline. It carries no `main` requirement
either. See [CLI Usage](../guides/cli-usage.md#emitting-llvm-ir).

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
neurc check   <INPUT>            # Stages 1-5: resolve + parse, bind, type-check, lower to HIR
neurc compile <INPUT> [OPTIONS]  # Full pipeline to a native binary
neurc run     <INPUT> [-O <N>]   # Compile into a temporary directory and run
```

**Options** (for `compile`):
- `-o, --output <FILE>`: output executable path (defaults to the input filename, `.exe` on Windows)
- `-O <LEVEL>`: optimization level (0 to 3)
- `--emit <exe|obj|llvm-ir>`: artifact to write (defaults to `exe`)

**Examples**:
```bash
neurc check   examples/basics/hello.nr
neurc compile examples/basics/hello.nr
neurc compile examples/basics/hello.nr -o bin/hello
RUST_LOG=debug neurc compile examples/basics/hello.nr   # debug logging
```

Every flag is described in the [CLI Usage Guide](../guides/cli-usage.md).

### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Compilation succeeded |
| 1 | Compilation failed (syntax, type, HIR-lowering, codegen, or link error) |

Under `run`, a successful build exits with the program's own status instead.

## Testing

End-to-end coverage lives in `compiler/neurc/tests/`. The per-feature suites under
`tests/suite/` (`arrays.rs`, `drop_destructors.rs`, `hir_lowering.rs`, …) compile and run real
programs, asserting exit codes and output. `tests/examples.rs` builds and runs every program in
`examples/` against its pinned exit code and stdout, and `tests/architecture_tests.rs` enforces
the slice dependency rules. `cargo test --workspace` runs all of it; the `mlir`-feature tests
are additional and feature-gated.

## Known Limitations

1. **Debug information**: no DWARF/PDB generation yet.
2. **Flat namespace**: module resolution merges every file into one namespace; two modules
   declaring the same top-level name collide rather than nesting.
3. **System toolchain required** for linking (`clang`/MSVC on Windows, `gcc`/`clang` on Unix); no
   bundled linker.

## Future Enhancements

Planned on the [Quick Roadmap](../../README.md#quick-roadmap): debug information (`-g`),
incremental compilation with a persistent cache, LTO defaults for release builds, and routing
tensor lowering through `mlir-backend` in the driver, then on to MLIR GPU dialects.

## Setup

LLVM 22 with `LLVM_SYS_221_PREFIX` set is required to build the compiler. See the
[Installation Guide](../getting-started/installation.md) for per-platform instructions (Linux,
macOS, Windows) and the optional MLIR backend setup. Common build problems are covered in
[Troubleshooting](../guides/troubleshooting.md).

## References

- [CONTRIBUTING.md](../../CONTRIBUTING.md), development guidelines and architecture rules
- [CHANGELOG.md](../../CHANGELOG.md), version history
- [compiler/neurc/src/main.rs](../../compiler/neurc/src/main.rs), implementation
- [Installation Guide](../getting-started/installation.md), toolchain setup
