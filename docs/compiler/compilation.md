# End-to-End Compilation

`neurc` turns a Neuro source file (`.nr`) into a native executable, an object file or textual LLVM IR. This page gives the stage order, what each stage hands the next, how failures are reported, and what `check`, `compile` and `run` do. Each stage's rules live on its component page, linked from the table below.

## Stages

Every command runs the same front half in the same order, so none can skip a step.

| # | Stage | Entry point | Hands the next stage |
|---|---|---|---|
| 1 | Read the root file | the driver (`compiler/neurc`) | Checks the `.nr` extension, then reads the source. A leading UTF-8 byte order mark is dropped. |
| 2 | [Lexical analysis](components/lexical-analysis.md) | `syntax_parsing::parse` | A token stream |
| 3 | [Parsing](components/syntax-parsing.md) | `syntax_parsing::parse` | An AST for each file |
| 4 | [Module resolution](components/module-resolution.md) | `module_resolution::resolve_program` | One flat item list covering every reachable module. The driver prepends the prelude unless the root file has `@no_prelude`. |
| 5 | [Argument binding](components/argument-binding.md) | `argument_binding::bind_arguments` | The same list with each named call in declaration order and no labels |
| 6 | [Semantic analysis](components/semantic-analysis.md) | `semantic_analysis::type_check` | A pass or the full set of type errors, plus lint warnings. The AST is not annotated. |
| 7 | [HIR lowering](components/hir-lowering.md) | `hir_lowering::lower_program` | Typed HIR (`neuro-hir`), where every expression carries its type and every tensor operation is outlined into a function of its own |
| 8 | [MLIR bodies](components/mlir-backend.md) | `mlir_backend::lower_for_link`, and `lower_for_gpu` for GPU code | Bodies for the outlined tensor functions, passed to the LLVM backend as external bodies it links in place of its own. GPU bodies are built only for a program with a `@gpu` or `@kernel` function, or a tensor operation that follows its operands, for the `--gpu-arch` chip. |
| 9 | [LLVM backend](components/llvm-backend.md) | `llvm_backend::compile`, or `compile_to_ir` under `--emit llvm-ir` | Object code, written to a temporary `.o` or `.obj` |
| 10 | Link | the platform linker driver (see [Linking](#linking)) | The executable |

The driver sets the order. Each rule lives in the slice that owns its stage. Argument binding and the type checker report every error they find. The other stages stop at the first failure.

The `-O` level reaches stage 8 and stage 9. At `-O0` the tensor bodies check integer overflow and panic on it, as the LLVM backend does. At `-O1` and above overflow wraps.

## Commands

**`neurc check <FILE>`** runs stages 1 to 7 and stops. It needs no `main`, and it does not invoke LLVM or the linker. On Linux, when the program has a `@gpu` or `@kernel` function, it also builds the GPU bodies for the default chip, so a body that cannot become a kernel fails `check` as it fails `compile`. On success it prints `Type checking passed for "<FILE>" (<N> module(s), <M> HIR items)`.

**`neurc compile <FILE>`** runs all ten stages. With the default `--emit exe` the program must define `main`. The driver checks this after HIR lowering and before stage 8, so a missing entry point is reported as a compile error instead of as the linker's `undefined reference to 'main'`. `--emit obj` and `--emit llvm-ir` need no `main`, since an object or IR module may be a library. They stop before linking and before instruction selection respectively. On success it prints `Successfully compiled <FILE> -> <OUTPUT>`. The output path may not be one of the program's own source files, and that is refused before anything is written.

**`neurc run <FILE>`** compiles as `compile` does, into a temporary directory, runs the executable and exits with its status. The executable keeps the source's name, so `argv[0]` is meaningful. The directory is removed on exit, and no success line is printed, so the program's stdout is its own.

Flags are documented in the [CLI Usage Guide](../guides/cli-usage.md), including `--gpu-arch`, which `compile` and `run` accept.

## Errors

Diagnostics go to stderr. A located error prints its message, the file, the line and column, the source line, and a caret under the span:

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

`compile` and `run` print the failing stage's error chain: `Compilation failed:` (or `Run failed:`) followed by one `Caused by` line per cause. `check` prints only the top message, as `Error: <message>`. A program with more than one module prints its messages without a source line, because spans from merged modules share one offset space (see [Module Resolution](components/module-resolution.md#flat-namespace)). Lint warnings go to stderr and never change the exit status.

| Command | Status | Meaning |
|---|---|---|
| `check` | 0 | Type checking passed |
| | 1 | An error in stages 1 to 7, or in the GPU check |
| `compile` | 0 | The artifact was written |
| | 1 | An error in any stage, including a missing `main` or a linker failure |
| `run` | the program's own status | The program ran; `main`'s return value, which the OS truncates to 8 bits |
| | 1 | Compilation or linking failed, or the program was killed by a signal |
| all | 2 | A rejected command line, such as an unknown flag or `-O4`. Nothing is read. |
| all | 101 | The compiler itself panicked. The panic message is already on stderr. |

## Linking

The driver cannot ship a linker. The object file needs a C compiler driver to attach the C runtime's startup code and to link the system libraries.

| Platform | Drivers, in order | Notes |
|---|---|---|
| Linux | `cc` (gcc or clang) | Adds `-lm` and `-ldl`. Needs a C compiler. |
| macOS | `cc` (clang) | Same flags as Linux. Provided by the Xcode Command Line Tools. |
| Windows | `clang`, then `lld-link`, then `cl.exe` | The first that links wins. When all fail, every driver's diagnosis is printed together, because an absent driver and one that ran and rejected the object mean different things. |

`-lm` covers the math functions tensor code calls, and `-ldl` the GPU runtime's `dlopen` on older glibc. Neither is passed on Windows.

## Building

The compiler needs LLVM 23 and MLIR, with the `LLVM_SYS_231_PREFIX`, `MLIR_SYS_230_PREFIX` and `TABLEGEN_230_PREFIX` variables set. See the [Installation Guide](../getting-started/installation.md).

## Testing

- [`compiler/neurc/tests/suite/`](../../compiler/neurc/tests/suite/) compiles and runs real programs per feature, asserting exit codes and output.
- [`cli_contract.rs`](../../compiler/neurc/tests/suite/cli_contract.rs) covers the command-line contract and the exit codes above.
- [`compiler/neurc/tests/examples.rs`](../../compiler/neurc/tests/examples.rs) builds and runs every program in `examples/` against its pinned exit code and stdout.
- [`compiler/neurc/tests/architecture_tests.rs`](../../compiler/neurc/tests/architecture_tests.rs) enforces the slice dependency rules.

`cargo test --workspace` runs all of it.

## Known Limitations

1. **Debug information**: no DWARF or PDB output, and no `-g` flag. It is on the [Quick Roadmap](../../README.md#quick-roadmap).
2. **Diagnostics in multi-module programs**: spans index their own file, and the driver cannot yet tell which one. A compile error in a program of more than one module prints no location, and a panic in a module other than the root reports a position in the root file's coordinates.
3. **System toolchain required**: linking needs `cc` on Unix, or clang, `lld-link` or MSVC on Windows. No linker is bundled.

## Source

- [`compiler/neurc/src/main.rs`](../../compiler/neurc/src/main.rs): the driver, the command line and the link step
- [`compiler/neurc/src/prelude.nr`](../../compiler/neurc/src/prelude.nr) and [`prelude.rs`](../../compiler/neurc/src/prelude.rs): the prelude the driver prepends
- [`compiler/neurc/CONTEXT.md`](../../compiler/neurc/CONTEXT.md): why the stages run in this order

## See also

- [CLI Usage](../guides/cli-usage.md) and [Troubleshooting](../guides/troubleshooting.md)
- [Installation Guide](../getting-started/installation.md)
- [CONTRIBUTING.md](../../CONTRIBUTING.md): development guidelines and architecture rules
