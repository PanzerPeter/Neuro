# CLI Usage Guide

Complete reference for the `neurc` command-line compiler.

## Installation

After building from source:

```bash
# Install compiler globally
cargo install --path compiler/neurc

# Or run directly from source
cargo run -p neurc -- <command> <args>
```

## Commands

### check

Validate syntax and types without generating code.

**Syntax**:
```bash
neurc check <file.nr>
```

**Examples**:
```bash
# Check a single file
neurc check examples/basics/hello.nr

# With debug logging
RUST_LOG=debug neurc check examples/basics/milestone.nr
```

**Output**:
- Success: `Type checking passed for "examples/basics/hello.nr" (1 module(s), 11 HIR items)`
- Failure: `Type errors found in "<file>":` followed by a numbered error list, then `Error: N type error(s) found`

**Exit codes**:
- 0: No errors found
- 1: Errors found

### compile

Compile Neuro source to native executable.

**Syntax**:
```bash
neurc compile <file.nr> [options]
```

**Options**:
- `-o, --output <FILE>` - Specify output executable path (default: the input filename without its extension)
- `-O, --optimization <0-3>` - Optimization level (default: `0`); see [Optimization](#optimization)

**Examples**:
```bash
# Default output (hello.exe on Windows)
neurc compile examples/basics/hello.nr

# Custom output path
neurc compile examples/basics/hello.nr -o bin/my_program

# Custom output with .exe extension
neurc compile examples/basics/hello.nr -o bin/my_program.exe

# Compile from different directory
neurc compile ../path/to/program.nr

# With debug logging
RUST_LOG=debug neurc compile examples/basics/hello.nr
```

**Output**:
- Success: `Successfully compiled <input.nr> -> <output_path>`
- Failure: the failing stage's errors, then `Compilation failed: <reason>` plus a numbered `Caused by (n):` chain

**Exit codes**:
- 0: Compilation successful
- 1: Compilation failed

## Environment Variables

### RUST_LOG

Control logging verbosity:

```bash
# Windows (PowerShell)
$env:RUST_LOG="debug"
neurc compile program.nr

# Unix
RUST_LOG=debug neurc compile program.nr
```

**Levels**:
- `error` - Only errors
- `warn` - Warnings and errors
- `info` - Informational messages
- `debug` - Detailed compilation steps (recommended for troubleshooting)
- `trace` - Very verbose output

**Examples**:
```bash
RUST_LOG=error neurc compile program.nr   # Minimal output
RUST_LOG=info neurc compile program.nr    # Standard output
RUST_LOG=debug neurc compile program.nr   # Detailed diagnostics
```

### LLVM_SYS_201_PREFIX

Path to the LLVM 20 installation (required to build the compiler):

```bash
# Arch / CachyOS
export LLVM_SYS_201_PREFIX=/usr/lib/llvm20

# Ubuntu / Debian
export LLVM_SYS_201_PREFIX=/usr/lib/llvm-20

# macOS (Homebrew)
export LLVM_SYS_201_PREFIX=$(brew --prefix llvm@20)
```

See the [Installation Guide](../getting-started/installation.md) for full setup.

## Workflow Examples

### Basic Development

```bash
# 1. Write code in editor
vim program.nr

# 2. Check for errors (fast)
neurc check program.nr

# 3. Fix errors, re-check
neurc check program.nr

# 4. Compile when ready
neurc compile program.nr

# 5. Run executable
./program  # Unix
.\program.exe  # Windows
```

### Rapid Iteration

```bash
# Check-compile-run loop (Unix)
neurc check program.nr && neurc compile program.nr && ./program

# Windows (PowerShell)
neurc check program.nr; if ($?) { neurc compile program.nr; if ($?) { .\program.exe } }
```

### Debugging Compilation Issues

```bash
# Enable debug logging
RUST_LOG=debug neurc compile program.nr 2> debug.log

# Review debug.log for detailed diagnostics
cat debug.log
```

## Output Files

### Executable

Default executable naming:
- **Windows**: `<input>.exe` (e.g., `program.nr` → `program.exe`)
- **Unix**: `<input>` without extension (e.g., `program.nr` → `program`)

Custom output with `-o`:
```bash
neurc compile program.nr -o custom_name
neurc compile program.nr -o bin/release/app
```

### Temporary Files

A temporary object file is written during compilation and deleted after linking:
- Location: system temp directory
- Format: `.o` on Unix, `.obj` on Windows
- Cleanup: removed once the linker finishes

## Error Handling

Errors go to stderr with exit code `1`. `compile` wraps the failing stage's message in a
`Compilation failed:` line followed by a numbered `Caused by (n):` chain.

### Parse Errors

Example:
```
Error: Module error: failed to parse module `bad.nr`: unexpected token RightBrace, expected expression
```

**Information provided**:
- The module that failed to parse
- The offending token and what the parser expected

### Type Errors

Example:
```
Type errors found in "bad.nr":
  1. type mismatch at Span { start: 25, end: 42 }: expected i32, found bool
Error: 1 type error(s) found
```

**Information provided**:
- A numbered list of every type error (the type checker keeps going after the first)
- Each error's kind, its byte range in the source (`Span`), and the expected/found types

### Code Generation and Link Errors

Codegen failures print as `Compilation failed: Code generation error: ...`; link failures
name the linker invocation, for example:

```
Compilation failed: Failed to link object file /tmp/neuro.o to executable program
Caused by (1): Failed to execute cc - ensure a C compiler (gcc/clang) is installed
```

**Common causes**:
- Missing C toolchain (MSVC or clang on Windows, gcc/clang on Unix)
- Missing system libraries

## Performance

### Compilation Times

Rough figures for a release-build compiler on a modern desktop; a debug build of `neurc`
itself is slower:

| Program Size | Check Time | Compile Time |
|--------------|------------|--------------|
| Small (<100 LOC) | <100ms | <1s |
| Medium (<1000 LOC) | <500ms | <5s |
| Large (<10000 LOC) | <2s | <30s |

### Optimization

`neurc compile` supports optimization levels `-O0` through `-O3`.

- `-O0`: Fastest compile time, minimal optimization
- `-O1`: Basic optimization
- `-O2`: Balanced optimization (recommended default for release-like builds)
- `-O3`: Maximum optimization

## Platform-Specific Notes

### Windows

**Requirements**:
- MSVC Build Tools 2022 OR MinGW-w64
- LLVM 20 (full development package)
- vcpkg with libxml2

**Executable extension**: Always `.exe`

**Path separators**: Use backslashes or forward slashes

```powershell
# Both work
neurc compile examples\hello.nr
neurc compile examples/basics/hello.nr
```

### Linux

**Requirements**:
- GCC or Clang
- LLVM 20
- Build essentials (make, cmake, etc.)

**Executable extension**: None (no extension)

**Permissions**: Make executable:
```bash
chmod +x ./program
./program
```

### macOS

**Requirements**:
- Xcode Command Line Tools
- LLVM 20 (via Homebrew, `llvm@20`)

**Apple Silicon**: Fully supported

**Executable extension**: None

## Advanced Usage

### Custom Toolchain

Override default linker:

```bash
# Use specific linker (not yet configurable in Phase 1)
# Future feature
```

### Cross-Compilation

Not yet supported in Phase 1. Compiles for native target only.

### Build Scripts

Integrate into build scripts:

```bash
#!/bin/bash
set -e

echo "Checking Neuro programs..."
neurc check src/main.nr
neurc check src/utils.nr

echo "Compiling..."
neurc compile src/main.nr -o bin/app

echo "Build complete!"
```

### CI/CD Integration

```yaml
# Example GitHub Actions workflow
- name: Install LLVM
  run: |
    wget https://apt.llvm.org/llvm.sh
    chmod +x llvm.sh
    sudo ./llvm.sh 20

- name: Build Neuro compiler
  run: cargo build --release -p neurc

- name: Compile program
  run: cargo run --release -p neurc -- compile program.nr

- name: Run tests
  run: ./program
```

## Troubleshooting

See [Troubleshooting Guide](troubleshooting.md) for common issues and solutions.

## Future Features

### Planned CLI Enhancements

- Debug information: `-g` flag
- Position-independent code: `-fPIC`
- Verbose output: `-v` flag
- Quiet mode: `-q` flag
- Color output control: `--color` option

### Planned

- Incremental compilation
- Build caching
- Cross-compilation targets

## References

- [Quick Start Guide](../getting-started/quick-start.md)
- [Troubleshooting Guide](troubleshooting.md)
- [Language Reference](../language-reference/types.md)
