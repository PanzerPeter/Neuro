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
neurc check examples/hello.nr

# With debug logging
RUST_LOG=debug neurc check examples/milestone.nr
```

**Output**:
- Success: "Type checking passed!"
- Failure: Detailed error messages with locations

**Exit codes**:
- 0: No errors found
- 1: Errors found

### compile

Compile NEURO source to native executable.

**Syntax**:
```bash
neurc compile <file.nr> [options]
```

**Options**:
- `-o, --output <FILE>` - Specify output executable path (default: same as input filename)

**Examples**:
```bash
# Default output (hello.exe on Windows)
neurc compile examples/hello.nr

# Custom output path
neurc compile examples/hello.nr -o bin/my_program

# Custom output with .exe extension
neurc compile examples/hello.nr -o bin/my_program.exe

# Compile from different directory
neurc compile ../path/to/program.nr

# With debug logging
RUST_LOG=debug neurc compile examples/hello.nr
```

**Output**:
- Success: "Compilation successful: <output_path>"
- Failure: Detailed error messages

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

### LLVM_SYS_181_PREFIX

Path to LLVM 18.1.8 installation (required):

```bash
# Windows
setx LLVM_SYS_181_PREFIX "C:\LLVM-1818"

# Unix
export LLVM_SYS_181_PREFIX=/usr/lib/llvm-18
```

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

Temporary object files are created during compilation but automatically deleted:
- Location: System temp directory
- Format: `.o` object files
- Cleanup: Automatic via RAII

## Error Handling

### Parse Errors

Example:
```
Parse error: unexpected token `}`, expected expression
  at examples/bad.nr:5:12
```

**Information provided**:
- Error type (Parse error)
- What went wrong (unexpected token)
- What was expected
- Exact location (file:line:column)

### Type Errors

Example:
```
Type error: Type mismatch
  expected: i32
  found: bool
  at examples/bad.nr:8:18
```

**Information provided**:
- Error type (Type error)
- Expected type
- Found type
- Exact location

### Code Generation Errors

Example:
```
Code generation error: undefined variable 'x'
  at examples/bad.nr:10:12
```

### Link Errors

Example:
```
Failed to link object file temp.o to executable program.exe
  Caused by: linker error: cannot find -lmsvcrt
```

**Common causes**:
- Missing C toolchain
- Missing system libraries
- Incorrect linker configuration

## Performance

### Compilation Times

Typical compilation times (Phase 1):

| Program Size | Check Time | Compile Time |
|--------------|------------|--------------|
| Small (<100 LOC) | <100ms | <1s |
| Medium (<1000 LOC) | <500ms | <5s |
| Large (<10000 LOC) | <2s | <30s |

### Optimization

Currently only `-O0` (no optimization) is supported. Higher optimization levels pending Phase 1.5.

## Platform-Specific Notes

### Windows

**Requirements**:
- MSVC Build Tools 2022 OR MinGW-w64
- LLVM 18.1.8 (full development package)
- vcpkg with libxml2

**Executable extension**: Always `.exe`

**Path separators**: Use backslashes or forward slashes

```powershell
# Both work
neurc compile examples\hello.nr
neurc compile examples/hello.nr
```

### Linux

**Requirements**:
- GCC or Clang
- LLVM 18
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
- LLVM 18 (via Homebrew)

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

echo "Checking NEURO programs..."
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
    sudo ./llvm.sh 18

- name: Build NEURO compiler
  run: cargo build --release -p neurc

- name: Compile program
  run: cargo run --release -p neurc -- compile program.nr

- name: Run tests
  run: ./program
```

## Troubleshooting

See [Troubleshooting Guide](troubleshooting.md) for common issues and solutions.

## Future Features

### Planned for Phase 1.5+

- Optimization levels: `-O1`, `-O2`, `-O3`
- Debug information: `-g` flag
- Position-independent code: `-fPIC`
- Verbose output: `-v` flag
- Quiet mode: `-q` flag
- Color output control: `--color` option

### Planned for Phase 2+

- Multiple file compilation
- Module system support
- Incremental compilation
- Build caching
- Cross-compilation targets

## References

- [Quick Start Guide](../getting-started/quick-start.md)
- [Troubleshooting Guide](troubleshooting.md)
- [Language Reference](../language-reference/types.md)
