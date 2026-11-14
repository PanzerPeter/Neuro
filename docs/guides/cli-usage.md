# CLI Usage Guide

`neurc` is the Neuro compiler's command-line driver. It checks, compiles and runs `.nr` files, and this page lists every command, flag and environment variable it reads.

## Installation

Build the compiler against an LLVM 23 and MLIR 23 prefix (see [Installation](../getting-started/installation.md)), then either install it or run it from the checkout:

```bash
cargo install --path compiler/neurc        # puts neurc on PATH
cargo run -p neurc -- <command> <args>     # or run it without installing
```

The build reads the three prefix variables under [Environment Variables](#environment-variables).

## Usage

`neurc --help` lists the commands, `neurc <command> --help` lists a command's flags, and `neurc --version` prints the compiler version. A rejected command line (an unknown flag, `-O4`, a `--gpu-arch` with neither prefix) exits with status `2` before any file is read.

## Commands

### check

Validate syntax and types without generating code. `check` takes no flags.

```bash
neurc check examples/basics/hello.nr
```

**Output**:
- Success: `Type checking passed for "<file>" (N module(s), N HIR items)` on stdout. The item count includes the prelude's items.
- Failure: the diagnostics on stderr, then `Error: N type error(s) found` (or `Error: Parsing failed` for a syntax error).

Lint warnings go to stderr and never change the exit status.

**Exit codes**: `0` no errors, `1` errors found.

### compile

Compile a Neuro source file to a native executable, an object file or LLVM IR.

```bash
neurc compile <file.nr> [options]
```

**Options**:
- `-o, --output <FILE>`: output path. Default: the input name without its extension (`hello` for `hello.nr`, `hello.exe` on Windows), `.o` or `.obj` with `--emit obj`, `.ll` with `--emit llvm-ir`.
- `-O, --optimization <0-3>`: optimization level, default `0`. See [Optimization levels](#optimization-levels).
- `--emit <exe|obj|llvm-ir>`: artifact to write, default `exe`. See [Emitting an object file](#emitting-an-object-file) and [Emitting LLVM IR](#emitting-llvm-ir).
- `--gpu-arch <CHIP>`: the GPU that `@gpu` kernels and `Device::GPU` tensors are built for, default `sm_60`. See [Choosing a GPU](#choosing-a-gpu). `run` takes it too.

```bash
neurc compile examples/basics/hello.nr                 # writes examples/basics/hello
neurc compile examples/basics/hello.nr -o bin/hello -O2
```

**Output**: `Successfully compiled <input.nr> -> <output>` on stdout. The object file of an executable build goes to a temporary directory that is removed after linking.

`-o` may not name a source file of the program: `neurc` refuses rather than overwrite it.

**Exit codes**: `0` success, `1` compilation or link failure, `2` a rejected command line.

#### Choosing a GPU

`--gpu-arch` names a chip, and the prefix picks the vendor. `sm_NN` is an NVIDIA compute capability. The kernels ship as PTX, which the CUDA driver compiles for any GPU at or above it, so the default `sm_60` covers every recent NVIDIA card. `gfxNNN` is an AMD architecture. The kernels are compiled to a code object for exactly that chip, and the program opens HIP (`libamdhip64.so`) instead of the CUDA driver.

```bash
neurc compile --gpu-arch gfx90a model.nr    # an MI200-series GPU
```

Compiling for AMD needs ROCm, because the code object is linked with ROCm's `ld.lld` (see [`ROCM_PATH`](#rocm_path)). Without it the compile fails and says so. Running needs the HIP runtime and a GPU of that architecture. A program that only moves tensors with `.to(Device::GPU(0))` links the same runtime, so the flag matters for it too. A chip name with neither prefix is refused by `neurc`, and a chip LLVM 23 has no processor for is refused by the MLIR lowering (`unknown GPU chip`).

#### Emitting an object file

`--emit obj` stops before the linker and writes the object file to the output path. An object may be a library, so it does **not** require a `main` function. The default `--emit exe` still does.

This is how a Neuro module is made callable from another language. A tensor value is a DLPack handle (see [tensors](../language-reference/tensors.md)), so a function returning `Tensor<T, S>` returns a `DLManagedTensorVersioned*` that NumPy, PyTorch or JAX consumes with no conversion step:

```bash
neurc compile --emit obj -o tensors.o tensors.nr
cc -shared -o libtensors.so tensors.o -lm
```

```python
import ctypes, numpy as np
lib = ctypes.CDLL("./libtensors.so")
lib.make_batch.restype = ctypes.c_void_p
```

`tools/dlpack_differential.py` is a worked version of this: it links a library of tensor-returning functions and checks every result against NumPy's own, reading dtype, rank, extents and strides from the handle. Run it with `python tools/dlpack_differential.py --neurc target/debug/neurc`, or through `cargo test -p neurc --test numpy_differential`.

`tools/grad_differential.py` links the same way and takes gradients of compiled scalar functions by central differences. Run it with `python tools/grad_differential.py --neurc target/debug/neurc`, or through `cargo test -p neurc --test grad_differential`.

`neurc` does not link the shared library itself: `-shared` is the platform C compiler's job, and on Windows a DLL additionally needs an export list.

#### Emitting LLVM IR

`--emit llvm-ir` stops before instruction selection and writes the textual module. Like `--emit obj` it does not require a `main` function.

```bash
neurc compile --emit llvm-ir -o twice.ll twice.nr
```

The module carries the host data layout and triple, so it can be handed straight to `opt`, `llc` or `clang`. `-O` still selects the middle-end pass pipeline: `-O0` emits the module as codegen built it, and `-O2` emits what the object path would have given the backend.

### run

Compile a source file to a temporary executable and run it immediately.

```bash
neurc run <file.nr> [options]
```

**Options**:
- `-O, --optimization <0-3>`: default `0`. See [Optimization levels](#optimization-levels).
- `--gpu-arch <CHIP>`: as for `compile`. See [Choosing a GPU](#choosing-a-gpu).

The executable is removed once the program exits, so `run` never leaves a binary beside the source. Use `compile` to keep one.

**Output**: the program's own stdout and stderr, with no compiler banner. Compiler and linker failures go to stderr as `Run failed: <reason>` followed by a `Caused by (n):` chain.

**Exit codes**:
- The program's own exit code, when it was built and started. `main`'s return value is the status, truncated to 8 bits by the OS.
- `1`: compilation, linking or process startup failed, or the program was killed by a signal.

## Optimization levels

`-O` applies to `compile` and `run`.

- `-O0` (default): a debug build. Integer overflow panics: `panic: integer overflow at <file>:<line>:<col>`.
- `-O1` to `-O3`: release builds. Integer overflow wraps. `-O2` is the usual choice for speed; `-O3` is the most aggressive.

Division by zero panics at every level. For the speed of the programs `neurc` produces, see the [benchmarks](../../README.md#performance).

## Diagnostics

Every diagnostic that names a location has the same shape: the message, ` --> file:line:column`, the source line and a caret under the span. The column counts characters. The file is the one the span is in, so an error in an imported module names that module. A lint warning has the same shape under a `warning[<lint>]` label. The examples of each kind of error are in [Troubleshooting](troubleshooting.md#compilation-errors).

- Parse errors stop at the first one and end with `Parsing failed`.
- Type errors are all reported, then `N type error(s) found`. A use-after-move adds a `note: moved here` line.
- Under `compile` and `run`, a failure is wrapped as `Compilation failed: <reason>` (or `Run failed: <reason>`), followed by a numbered `Caused by (n):` chain.

### Code generation and link errors

A codegen failure prints as `Compilation failed: Code generation error: ...`. A link failure names the linker invocation:

```text
Compilation failed: Failed to link object file to executable
  Caused by (1): Failed to execute cc - ensure a C compiler (gcc/clang) is installed
  Caused by (2): No such file or directory (os error 2)
```

On Unix the linker driver is `cc`, and every Unix link passes `-lm -ldl`. On Windows `neurc` tries `clang`, then `lld-link`, then MSVC's `cl.exe`, and when all three fail it reports each driver's own reason.

## Environment Variables

### RUST_LOG

Sets the log level for `neurc`'s own messages, printed to stderr as `[INFO  neurc] ...`:

| Level | Shows |
| --- | --- |
| `error` | errors only |
| `warn` | warnings and errors |
| `info` | the file being compiled and the optimization level |
| `debug` | each pipeline stage (module resolution, type checking, lowering, codegen, linking) |
| `trace` | everything |

```bash
RUST_LOG=debug neurc compile program.nr 2> debug.log      # Unix
$env:RUST_LOG="debug"; neurc compile program.nr 2> debug.log   # PowerShell
```

### ROCM_PATH

Read when compiling for an AMD chip (`--gpu-arch gfxNNN`). The code object is linked with `$ROCM_PATH/llvm/bin/ld.lld`, and `/opt/rocm` is used when it is unset.

### Build-time prefix variables

`LLVM_SYS_231_PREFIX`, `MLIR_SYS_230_PREFIX` and `TABLEGEN_230_PREFIX` are read by Cargo while building `neurc`, not by `neurc` itself. All three must point at the one prefix that holds LLVM 23 and MLIR 23:

```bash
prefix=/opt/llvm-mlir-23                  # Arch / CachyOS (source build)
prefix=/usr/lib/llvm-23                   # Ubuntu / Debian (apt.llvm.org)
prefix=$(brew --prefix llvm)              # macOS
export LLVM_SYS_231_PREFIX=$prefix MLIR_SYS_230_PREFIX=$prefix TABLEGEN_230_PREFIX=$prefix
```

The per-platform setup is in [Installation](../getting-started/installation.md).

## Platform Notes

- **Linux**: GCC or Clang on `PATH` (`build-essential` on Debian and Ubuntu, `base-devel` on Arch), plus the LLVM 23 prefix.
- **macOS**: Xcode Command Line Tools and LLVM 23 from Homebrew (`llvm`). Apple Silicon is supported.
- **Windows**: MSVC Build Tools 2022, and an LLVM 23 and MLIR 23 prefix built from source, which the [Windows section of Installation](../getting-started/installation.md#windows-msvc) describes. The executable always gets a `.exe` extension. Forward slashes and backslashes both work in paths.

`neurc` compiles for the host target only.

## Workflows

Check often, compile when the check passes:

```bash
neurc check program.nr && neurc compile program.nr && ./program          # Unix
neurc check program.nr; if ($?) { neurc compile program.nr; if ($?) { .\program.exe } }   # PowerShell
```

In a CI job, install `neurc` as above, then let the shell stop on the first failure:

```yaml
- name: Build and run
  run: |
    neurc check src/main.nr
    neurc compile src/main.nr -o app
    ./app
```

## Planned

Debug information and incremental compilation are planned. See the [Quick Roadmap](../../README.md#quick-roadmap).

## See also

- [Installation](../getting-started/installation.md)
- [Quick Start](../getting-started/quick-start.md)
- [Troubleshooting](troubleshooting.md)
- [Tensors](../language-reference/tensors.md), for the DLPack interop behind `--emit obj`
