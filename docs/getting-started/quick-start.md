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
```
Type checking passed for "examples/basics/hello.nr" (1 module(s), 11 HIR items)
```

## Compiling a Program

Compile a Neuro program to a native executable:

```bash
cargo run -p neurc -- compile examples/basics/hello.nr
```

The compiler prints the paths it produced:

```
Successfully compiled examples/basics/hello.nr -> examples/basics/hello
```

On Windows the executable is `examples\basics\hello.exe`; on Unix it is `examples/basics/hello`.

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

```
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
    val calculated: i32 = calculate(sum)
    return calculated
}
```

**Features demonstrated**:
- Function definitions with parameters and return types
- Calling functions and chaining their results
- Immutable variables (`val`)
- Integer arithmetic
- `return` statements (the program exits with the value `main` returns, here `26`)

### milestone.nr

The source of [`examples/basics/milestone.nr`](../../examples/basics/milestone.nr):

```neuro
func add(a: i32, b: i32) -> i32 {
    return a + b
}

func main() -> i32 {
    val result = add(5, 3)
    return result
}
```

**Features demonstrated**:
- Multiple functions in one file
- Function calls with arguments
- Type inference: `result` gets its type from the call's return type

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
- `-o, --output <FILE>` - Specify output executable path (default: the input filename without its extension, `.exe` on Windows)
- `-O, --optimization <0-3>` - Optimization level (default: `0`)

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
```
Error: Module error: failed to parse module `bad.nr`: unexpected token RightBrace, expected expression
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
```
Type errors found in "mismatch.nr":
  1. type mismatch at Span { start: 25, end: 42 }: expected i32, found bool
Error: 1 type error(s) found
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
neurc compile examples/basics/hello.nr

# Unix
RUST_LOG=debug neurc compile examples/basics/hello.nr
```

This shows each stage as it runs: module resolution and parsing, type checking, HIR lowering, LLVM IR and object-code generation, and linking.

## Current Feature Summary

Per-sub-phase status lives in the [Quick Roadmap](../../README.md#quick-roadmap). The
current compiler supports:

### Types
- Integers: `i8`, `i16`, `i32`, `i64`, `u8`, `u16`, `u32`, `u64`
- Floats: `f16`, `bf16`, `f32`, `f64`
- Boolean: `bool`; `char` (32-bit Unicode scalar)
- Strings: fat-pointer `string` with escape sequences (`\n`, `\t`, `\"`, `\\`, `\xNN`, `\u{NNNN}`); `==`/`!=` byte-level comparison; `+` concatenation; `.len()` / `.clone()` / `.slice(a..b)` (bytes) / `.char_slice(a..b)` (code points); `.chars()` iterates the scalars and `for (offset, c) in s.char_indices()` binds each one's byte offset
- Structs: user-defined types with nominal typing
- Fixed-size arrays `[T; N]`, tuples `(T1, T2, ...)`, enums with associated data, `newtype`, `type` aliases

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
- `impl` blocks with `&self` / `&mut self` methods and `TypeName::func` associated functions
- Generic functions, structs and impls with enforced trait bounds, const generics, `where` clauses, turbofish
- Closures and lambdas `|x: i32| x * x`; function type `(T1, ...) -> R`; higher-order functions

### Traits & Dispatch
- `trait` declarations with required and default methods; `impl Trait for Type`
- Static dispatch via `impl Trait` and trait-bounded generics (monomorphized)
- Dynamic dispatch via `&dyn Trait` (vtable-backed)
- Operator overloading through the compiler-known operator traits

### Ownership
- Move-by-default with use-after-move detection; `@derive(Copy, Clone)`; `.clone()`
- Immutable `&T` and mutable `&mut T` borrows with `*` deref; flow-sensitive borrow exclusivity
- Explicit lifetime annotations `<'a>`; returned-reference lifetime elision
- Deterministic `Drop` running at scope exit in reverse declaration order

### Control Flow
- `if` / `else if` / `else` chains; `if` and blocks as value expressions
- `while` loops; `loop` (including as a value expression)
- Range-for loops: `for i in 0..n` (exclusive) and `for i in 0..=n` (inclusive)
- `for x in e` over any type implementing the prelude's `IntoIterator` / `Iterator` protocol,
  adapters included
- `break` and `continue`, with value-carrying breaks and loop labels
- `match` as an exhaustive expression with payload binding, or-patterns, ranges, and guards
- `panic(msg)` / `assert(cond)` / `unreachable()`

### Operators
- Arithmetic: `+`, `-`, `*`, `/`, `%`
- Comparison: `==`, `!=`, `<`, `>`, `<=`, `>=`
- Logical: `&&`, `||`, `!`
- Bitwise: `&`, `|`, `^`, `~`, `<<` (integer types only)
- Compound assignment: `+=`, `-=`, `*=`, `/=`, `%=`
- Type cast: `as` for numeric conversions and bool-to-int
- Coalescing: `??` unwraps an `Option` / `Result`, else evaluates a lazy fallback
- Propagation: `expr?` unwraps an `Option` / `Result`, else returns the failure to the caller

### Modules
- A program may span several files: every `.nr` file is a module, and a directory holding a `mod.nr` is a module with children. You compile the root
- `import math`, `import ./utils::io`, `import math::{sqrt, sin}`, `as` renames, module aliases, variant imports, and `export import` re-export facades
- Inline `module Name { ... }` blocks group items inside one file, under the same rules
- Declarations and struct fields are private to their module until `export` opts them in
- An implicit prelude puts `Option`, `Result`, and `Some` / `None` / `Ok` / `Err` in scope in every file with no `import`; `@no_prelude` on a file's first line opts out
- Named arguments: `connect("localhost", port: 8080)`, in any order after the positional
  ones. A parameter declared `external internal: T` *requires* the external name at the
  call site; one declared `_ internal: T` is positional-only
- Triple-quoted `"""` block strings dedented to the closing delimiter's column, and block
  comments that nest

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
