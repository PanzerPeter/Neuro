# Quick Start Guide

Get up and running with Neuro in 5 minutes.

## Prerequisites

Ensure you have completed the [Installation Guide](installation.md) before proceeding.

## Your First Command

Check that the compiler is installed:

```bash
cargo run -p neurc -- --version
```

Or if you installed it globally:

```bash
neurc --version
```

## Checking a Program

Neuro can validate syntax and types without compiling:

```bash
cargo run -p neurc -- check examples/basics/hello.nr
```

Expected output:
```text
Type checking passed for "examples/basics/hello.nr" (1 module(s), 19 HIR items)
```

## Compiling a Program

Compile a Neuro program to a native executable:

```bash
cargo run -p neurc -- compile examples/basics/hello.nr
```

The compiler prints the paths it produced:

```text
Successfully compiled examples/basics/hello.nr -> examples/basics/hello
```

On Windows the executable is `examples\basics\hello.exe`; on Unix it is `examples/basics/hello`.

## Running the Executable

Execute the compiled program:

```bash
# Windows
.\examples\basics\hello.exe

# Unix
./examples/basics/hello
```

Check the exit code:

```bash
# Windows (PowerShell)
echo $LASTEXITCODE

# Unix
echo $?
```

The hello.nr program prints two lines and exits with 26.

A program reports a result two ways, and every example in this repository is verified
on both. `main`'s `i32` becomes the exit code, pinned in
[`examples/expected.txt`](../../examples/expected.txt); whatever the program writes to
standard output is pinned byte for byte in a sibling `.out` file. For text, `print` and
`println` write to standard output:

```neuro
func main() -> i32 {
    val name: string = "Neuro"
    print("hello from ")
    println(name)
    return 0
}
```

```text
hello from Neuro
```

Each takes one `string`, and interpolation renders the holes before the call, so
`println("phase {n} of {total}")` needs no format arguments. See
[`examples/basics/greeting.nr`](../../examples/basics/greeting.nr) for a runnable version
and the [functions reference](../language-reference/functions.md#standard-output-builtins)
for the full contract.

## Understanding the Examples

### hello.nr

The source of [`examples/basics/hello.nr`](../../examples/basics/hello.nr):

```neuro
func add(a: i32, b: i32) -> i32 {
    return a + b
}

func calculate(x: i32) -> i32 {
    val doubled: i32 = x * 2
    val result: i32 = doubled + 10
    return result
}

func main() -> i32 {
    val x: i32 = 5
    val y: i32 = 3

    val sum: i32 = add(x, y)
    println("add({x}, {y})    = {sum}")

    val calculated: i32 = calculate(sum)
    println("calculate({sum}) = {calculated}")

    return calculated
}
```

**Features demonstrated**:
- Function definitions with parameters and return types
- Calling functions and chaining their results
- Immutable variables (`val`)
- Integer arithmetic
- String interpolation with `println`
- `return` statements (the program exits with the value `main` returns, here `26`)

### function_call.nr

The source of [`examples/basics/function_call.nr`](../../examples/basics/function_call.nr):

```neuro
func add(a: i32, b: i32) -> i32 {
    return a + b
}

func main() -> i32 {
    val result = add(5, 3)
    println("add(5, 3) = {result}")
    return result
}
```

**Features demonstrated**:
- Multiple functions in one file
- Function calls with arguments
- Type inference: `result` gets its type from the call's return type

Compile and run:

```bash
cargo run -p neurc -- compile examples/basics/function_call.nr

# Windows
.\examples\basics\function_call.exe

# Unix
./examples/basics/function_call
```

Prints `add(5, 3) = 8` and exits with 8.

## CLI Options

### Check Command

```bash
neurc check <file.nr>
```

Validates syntax and types without generating code. Fast feedback for development.

### Compile Command

```bash
neurc compile <file.nr> [options]
```

**Options**:
- `-o, --output <FILE>`: output path (default: the input filename without its extension, `.exe` on Windows)
- `-O, --optimization <0-3>`: optimization level (default: `0`)
- `--emit <exe|obj|llvm-ir>`: what to write, a linked executable (default), an unlinked object
  file, or textual LLVM IR. See the [CLI guide](../guides/cli-usage.md)

**Examples**:

```bash
# Default output (same name as source)
neurc compile examples/basics/hello.nr

# Custom output path
neurc compile examples/basics/hello.nr -o bin/my_program

# Optimized build
neurc compile -O2 examples/basics/hello.nr

# Compile from a different directory
neurc compile ../path/to/program.nr
```

### Run Command

```bash
neurc run <file.nr> [options]
```

Compiles into a temporary directory, runs the program, and exits with the program's own
status. Nothing is written beside the source, so this is the command to reach for while
iterating; use `compile` when you want to keep the binary.

**Options**:
- `-O, --optimization <0-3>`: optimization level (default: `0`)

```bash
neurc run examples/basics/hello.nr
```

## Error Messages

Errors print to stderr and the compiler exits with code `1`.

### Syntax Error Example

Source (`bad.nr`):
```neuro
func main() -> i32 {
    val x: i32 =
}
```

Error output:
```text
error: unexpected token RightBrace, expected expression
 --> bad.nr:3:1
  |
3 | }
  | ^

Error: Parsing failed
```

### Type Error Example

Source (`mismatch.nr`):
```neuro
func main() -> i32 {
    val x: i32 = true
    return x
}
```

Error output:
```text
Type errors found in "mismatch.nr":
error: type mismatch: expected i32, found bool
 --> mismatch.nr:2:5
  |
2 |     val x: i32 = true
  |     ^^^^^^^^^^^^^^^^^

Error: 1 type error(s) found
```

## Development Workflow

1. **Write** your Neuro code in a `.nr` file
2. **Check** syntax and types: `neurc check program.nr`
3. **Run** it: `neurc run program.nr`
4. **Ship** a binary when you want one: `neurc compile program.nr`
5. **Iterate**: fix errors and repeat

### Recommended Workflow

For faster iteration during development:

```bash
# Check only (faster, no code generation)
neurc check program.nr

# Compile and run in one step, leaving no binary behind
neurc run program.nr

# When you want to keep the executable
neurc compile program.nr && ./program
```

## Debug Logging

Enable debug output to see compilation stages:

```bash
# Windows (PowerShell)
$env:RUST_LOG="debug"
neurc compile examples/basics/hello.nr

# Unix
RUST_LOG=debug neurc compile examples/basics/hello.nr
```

This shows each stage as it runs: module resolution and parsing, type checking, HIR lowering, LLVM IR and object-code generation, and linking.

## What the Language Covers

The [capability table](../../README.md#current-capabilities) lists what the compiler supports
today, and the [Quick Roadmap](../../README.md#quick-roadmap) what is planned. Each feature is
defined once, in the [language reference](../README.md#language-reference).

## Common Issues

### Compilation succeeds but linking fails

**Problem**: Missing C toolchain.

**Solution**: Install C compiler (MSVC on Windows, GCC/Clang on Unix).

### "Permission denied" when running executable

**Problem**: Execute permission not set (Unix).

**Solution**:
```bash
chmod +x ./program
./program
```

### Slow compilation

**Problem**: Building from source in debug mode.

**Solution**: Use release build for better performance:
```bash
cargo build --release -p neurc
cargo run --release -p neurc -- compile program.nr
```

## Next Steps

- [Your First Program](first-program.md): a detailed tutorial
- [Language Reference](../language-reference/types.md): the full language
- [CLI Usage Guide](../guides/cli-usage.md): every command and flag
- [Troubleshooting](../guides/troubleshooting.md): common problems and solutions

## Getting Help

- Check [Troubleshooting Guide](../guides/troubleshooting.md)
- Read [Language Reference](../language-reference/types.md)
- Report issues: https://github.com/PanzerPeter/Neuro/issues
- Read [CONTRIBUTING.md](../../CONTRIBUTING.md) for development guidelines
