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
cargo run -p neurc -- check examples/hello.nr
```

Expected output:
```
Type checking passed!
```

## Compiling a Program

Compile a Neuro program to a native executable:

```bash
cargo run -p neurc -- compile examples/hello.nr
```

On Windows, this creates `examples\hello.exe`.
On Unix, this creates `examples/hello`.

## Running the Executable

Execute the compiled program:

```bash
# Windows
.\examples\hello.exe

# Unix
./examples/hello
```

Check the exit code:

```bash
# Windows (PowerShell)
echo $LASTEXITCODE

# Unix
echo $?
```

The hello.nr program returns 26.

## Understanding the Examples

### hello.nr

A minimal Neuro program:

```neuro
func main() -> i32 {
    val x: i32 = 10
    val y: i32 = 16
    return x + y  // Returns 26
}
```

**Features demonstrated**:
- Function definition with return type
- Immutable variables (val)
- Integer arithmetic
- Return statements

### milestone.nr

A more complex program demonstrating Phase 1 capabilities:

```neuro
func add(a: i32, b: i32) -> i32 {
    return a + b
}

func main() -> i32 {
    val result: i32 = add(3, 5)
    if result > 5 {
        return result
    } else {
        return 0
    }
}
```

**Features demonstrated**:
- Multiple functions
- Function calls with parameters
- Local variables
- If/else control flow
- Comparison operators

Compile and run:

```bash
cargo run -p neurc -- compile examples/basics/milestone.nr

# Windows
.\examples\milestone.exe

# Unix
./examples/milestone
```

Exit code: 8

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
- `-o, --output <FILE>` - Specify output executable path

**Examples**:

```bash
# Default output (same name as source)
neurc compile examples/hello.nr

# Custom output path
neurc compile examples/hello.nr -o bin/my_program

# Compile from different directory
neurc compile ../path/to/program.nr
```

## Error Messages

Neuro provides detailed error messages with source locations.

### Syntax Error Example

Source:
```neuro
func bad() -> i32 {
    return   // Missing return value
}
```

Error output:
```
Parse error: unexpected token `}`, expected expression
  at examples/bad.nr:2:12
```

### Type Error Example

Source:
```neuro
func mismatch() -> i32 {
    val x: i32 = true  // Type mismatch
    return x
}
```

Error output:
```
Type error: Type mismatch
  expected: i32
  found: bool
  at examples/mismatch.nr:2:18
```

## Development Workflow

1. **Write** your Neuro code in a `.nr` file
2. **Check** syntax and types: `neurc check program.nr`
3. **Compile** to executable: `neurc compile program.nr`
4. **Run** the program: `./program` (Unix) or `.\program.exe` (Windows)
5. **Iterate** - fix errors and repeat

### Recommended Workflow

For faster iteration during development:

```bash
# Check only (faster, no code generation)
neurc check program.nr

# When ready, compile and run
neurc compile program.nr && ./program
```

## Debug Logging

Enable debug output to see compilation stages:

```bash
# Windows (PowerShell)
$env:RUST_LOG="debug"
neurc compile examples/hello.nr

# Unix
RUST_LOG=debug neurc compile examples/hello.nr
```

This shows:
- Lexical analysis progress
- Parse tree structure
- Type checking steps
- LLVM IR generation
- Linking process

## Current Feature Summary

Phase 1 is complete. Phase 1.5 and Phase 2 are in progress. The current compiler supports:

### Types
- Integers: `i8`, `i16`, `i32`, `i64`, `u8`, `u16`, `u32`, `u64`
- Floats: `f32`, `f64`
- Boolean: `bool`
- Strings: `string` literals with escape sequences (`\n`, `\t`, `\"`, `\\`, `\xNN`, `\u{NNNN}`); `==`/`!=` for byte-level comparison
- Structs: user-defined types with nominal typing

### Variables & Constants
- Immutable variables: `val x: i32 = 10`
- Mutable variables: `mut counter: i32 = 0`
- Variable reassignment: `counter = counter + 1`
- Compile-time constants: `const MAX: i32 = 100` at module and function scope
- Contextual numeric literal inference (e.g. `val n = 42` infers `i32`)

### Functions
- Function definitions with typed parameters
- Explicit `return` statements
- Expression-based implicit returns (trailing expression)
- Recursion and forward references
- `impl` blocks with `&self` methods and `TypeName::func` associated functions

### Control Flow
- `if` / `else if` / `else` chains
- `while` loops
- Range-for loops: `for i in 0..n` (exclusive) and `for i in 0..=n` (inclusive)
- `break` and `continue`

### Operators
- Arithmetic: `+`, `-`, `*`, `/`, `%`
- Comparison: `==`, `!=`, `<`, `>`, `<=`, `>=`
- Logical: `&&`, `||`, `!`
- Bitwise: `&`, `|`, `^`, `~`, `<<` (integer types only)
- Compound assignment: `+=`, `-=`, `*=`, `/=`, `%=`
- Type cast: `as` for numeric conversions and bool-to-int

### Not Yet Implemented (Phase 1.5 / Phase 2)
- Ownership and borrow checker (Phase 1.5)
- If/else as assignable expression: `val x = if c { a } else { b }` (Phase 1.5)
- String interpolation (Phase 1.5)
- Enums, pattern matching, tuples (Phase 2)
- Module system and imports (Phase 2)
- Generic functions and traits (Phase 2)

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

- [Your First Program](first-program.md) - Detailed tutorial
- [Language Reference](../language-reference/types.md) - Complete language documentation
- [CLI Usage Guide](../guides/cli-usage.md) - Advanced CLI features
- [Troubleshooting](../guides/troubleshooting.md) - Common problems and solutions

## Getting Help

- Check [Troubleshooting Guide](../guides/troubleshooting.md)
- Read [Language Reference](../language-reference/types.md)
- Report issues: https://github.com/PanzerPeter/Neuro/issues
- Read [CONTRIBUTING.md](../../CONTRIBUTING.md) for development guidelines
