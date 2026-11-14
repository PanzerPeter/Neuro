# Troubleshooting Guide

Fixes for the errors people hit when building the compiler and when running `neurc`. Each message quoted here is what `neurc` prints today. Anything not listed is worth searching for in [GitHub Issues](https://github.com/PanzerPeter/Neuro/issues) before you report it.

## Installation Issues

### "No suitable version of LLVM was found"

**Symptoms**:
```text
error: No suitable version of LLVM was found system-wide or pointed
       to by LLVM_SYS_231_PREFIX.
```

**Cause**: one of the three prefix variables is unset, or points at a prefix without LLVM 23 and MLIR 23 in it. The message names only the LLVM variable, but the other two are needed too.

**Solution**: point all three at the same prefix, as [Installation](../getting-started/installation.md) describes for your platform:

```bash
export LLVM_SYS_231_PREFIX=<prefix>
export MLIR_SYS_230_PREFIX=<prefix>
export TABLEGEN_230_PREFIX=<prefix>
```

Verify that the prefix holds LLVM with `ls "$LLVM_SYS_231_PREFIX/lib/cmake/llvm/LLVMConfig.cmake"`.

On Windows, the `LLVM-*-win64.exe` installer lacks the development files and MLIR. Build the prefix from source as the [Windows section of Installation](../getting-started/installation.md#windows-msvc) describes, and set the variables with `setx` as it shows.

### Linker `cc` not found (Unix)

**Symptoms**: building the compiler fails with

```text
error: linker `cc` not found
```

and `neurc compile` fails with

```text
Compilation failed: Failed to link object file to executable
  Caused by (1): Failed to execute cc - ensure a C compiler (gcc/clang) is installed
```

**Cause**: no C compiler is on `PATH`. `cc` is the linker driver for both Cargo and `neurc`.

**Solution**:

```bash
sudo apt-get install build-essential    # Ubuntu / Debian
sudo pacman -S base-devel               # Arch
xcode-select --install                  # macOS
```

### `mlir-sys` fails to build

Every build compiles the `mlir-backend` slice, which needs MLIR 23 on top of LLVM 23.

**Symptoms**:
```text
failed to run `".../bin/llvm-config" ...`
# or
fatal error: 'mlir-c/IR.h' file not found
# or, at link time (GNU ld)
cannot find -lMLIR-C: No such file or directory
```

**Cause**: `mlir-sys` finds MLIR by running `$MLIR_SYS_230_PREFIX/bin/llvm-config` and reading its `lib/` and `include/`. MLIR has to live in the same prefix as that `llvm-config`. A prefix with no static LLVM libraries makes `mlir-sys` link the shared `MLIR-C` library, which distro packages (Arch's `aur/mlir` included) do not build.

**Solution**: use a prefix that holds LLVM and MLIR together: apt.llvm.org's `/usr/lib/llvm-23` with `libmlir-23-dev` installed, Homebrew's `llvm`, or a source build of LLVM 23 with `-DLLVM_ENABLE_PROJECTS=mlir`. Point all three variables at it, as shown under [Installation Issues](#no-suitable-version-of-llvm-was-found). See [Installation → MLIR](../getting-started/installation.md#mlir).

If bindgen reports opaque 1-byte structs (`attempt to compute 0_usize - 8_usize`), the libclang it loaded is a different major version from the MLIR headers. Point `LIBCLANG_PATH` at a libclang 23.

## Compilation Errors

Diagnostics print the file, line and column, the source line, and a caret under the span. Type errors are all reported; a syntax error stops at the first one.

### Type Mismatch

**Symptoms**:
```text
Type errors found in "program.nr":
error: type mismatch: expected i32, found f64
 --> program.nr:2:5
  |
2 |     val x: i32 = 3.14
  |     ^^^^^^^^^^^^^^^^^

Error: 1 type error(s) found
```

**Cause**: Neuro does not convert between numeric types implicitly. `i32 + i64`, passing an `f64` to an `i32` parameter, and returning the wrong type are all mismatches.

**Solution**: declare the value with the type it has, or convert it explicitly with `as`:

```neuro
func main() -> i32 {
    val x: i32 = 3.14  // error: type mismatch: expected i32, found f64
    return x
}
```

```neuro
func main() -> i32 {
    val x: i32 = 3.14 as i32
    return x
}
```

### Undefined Variable

**Symptoms**:
```text
Type errors found in "program.nr":
error: undefined variable 'x'
 --> program.nr:5:12
  |
5 |     return x
  |            ^

Error: 1 type error(s) found
```

**Causes**: the variable is declared later, is misspelled, or was declared inside a block that has already closed.

**Solution**: declare it in the enclosing scope, before the block that needs it:

```neuro
func scoped() -> i32 {
    if true {
        val x: i32 = 10
    }
    return x  // error: undefined variable 'x'
}
```

```neuro
func scoped() -> i32 {
    val x: i32 = 10
    if true {
        return x + 1
    }
    return x
}
```

### Assignment to an Immutable Variable

**Symptoms**:
```text
Type errors found in "program.nr":
error: cannot assign to immutable variable 'x'
 --> program.nr:3:5
  |
3 |     x = 20
  |     ^

Error: 1 type error(s) found
```

**Cause**: `val` bindings cannot be reassigned.

**Solution**: declare the binding with `mut`:

```neuro
func main() -> i32 {
    val x: i32 = 10
    x = 20  // error: cannot assign to immutable variable 'x'
    return x
}
```

```neuro
func main() -> i32 {
    mut x: i32 = 10
    x = 20
    return x
}
```

### Missing Return Statement

**Symptoms**:
```text
Type errors found in "program.nr":
error: missing return statement in function returning i32
 --> program.nr:1:6
  |
1 | func compute(x: i32) -> i32 {
  |      ^^^^^^^

Error: 1 type error(s) found
```

The caret is under the function's name.

**Cause**: a path through the function body does not return a value.

**Solutions**: return on every path, or make the last expression the value:

```neuro
func bad(x: i32) -> i32 {  // error: missing return statement in function returning i32
    if x > 0 {
        return x
    }
}
```

```neuro
func good(x: i32) -> i32 {
    if x > 0 {
        return x
    } else {
        return 0
    }
}

func good_implicit(x: i32) -> i32 {
    if x > 0 {
        x
    } else {
        0
    }
}
```

### Missing `main` Function

**Symptoms**:
```text
Compilation failed: no `main` function found in program.nr: an executable needs an entry point
```

**Cause**: `compile` and `run` build an executable, which needs `func main() -> i32`. `check` does not require it.

**Solution**: add the entry point, or build a library with `--emit obj` or `--emit llvm-ir`, which do not require one (see [CLI Usage](cli-usage.md#emitting-an-object-file)).

### Parse Errors

**Symptoms**:
```text
error: unexpected token RightBrace, expected expression
 --> program.nr:2:18
  |
2 |     val x: i32 = }
  |                  ^

Error: Parsing failed
```

**Common causes**:

**A stray semicolon.** Neuro statements end at a newline. A `;` is a parse error:

```neuro
func main() -> i32 {
    val x: i32 = 10;  // error: unexpected token Semicolon, expected a newline or '}' to end the statement
    return x
}
```

Remove it: `val x: i32 = 10`.

**An unclosed brace.** The parser then reads the next `func` where it expects a statement:

```neuro
func bad() -> i32 {
    if true {
        return 1
    // missing closing brace for the `if`

func main() -> i32 {  // error: unexpected token Func, expected expression
    return bad()
}
```

Close the block before the next declaration.

## Runtime Issues

### Executable Does Not Run (Windows)

**Symptoms**: the executable is created but will not start, or reports a missing DLL.

**Causes**:
1. The Visual C++ runtime is missing.
2. Antivirus blocks the new executable.

**Solutions**: install the [Visual C++ Redistributable](https://aka.ms/vs/17/release/vc_redist.x64.exe) and restart, or add an antivirus exception for the executable.

### Permission Denied (Unix)

**Symptoms**:
```text
bash: ./program: Permission denied
```

**Cause**: the file lost its execute bit, usually when it was copied from a filesystem that does not keep permissions.

**Solution**: `chmod +x ./program`.

### Panics and Aborts

**Symptoms**: the program prints a located message and aborts. A compiled program exits with status `134` (`SIGABRT`). Under `neurc run` the status is `1`.

```text
panic: division by zero at program.nr:4:12
```

The messages are:
- `panic: division by zero at <file>:<line>:<col>`: an integer division or remainder by zero. Checked at every optimization level.
- `panic: integer overflow at <file>:<line>:<col>`: integer overflow in a `-O0` build. From `-O1` up, arithmetic wraps silently.
- `panic: <message> at <file>:<line>:<col>`: an explicit `panic(message)`.
- `assertion failed at <file>:<line>:<col>`: a failed `assert(cond)`.
- `internal error: entered unreachable code at <file>:<line>:<col>`: a reached `unreachable()`.

**Solution**: guard the divisor, or report the failure through an `Option` or `Result`:

```neuro
func safe_divide(a: i32, b: i32) -> i32 {
    if b == 0 {
        return 0  // or report the failure through an Option or Result
    }
    return a / b
}
```

For overflow, `.checked_add`, `.checked_sub` and `.checked_mul` return an `Option` instead of panicking; see [integer methods](../language-reference/types.md#integer-methods).

### Crashes (Signals)

A program that dies on a signal such as `SIGSEGV` with no `panic:` line has either recursed without a base case (which exhausts the stack) or hit a compiler bug. If the recursion is intended, check the base case first. Otherwise report it with a minimal reproduction at [GitHub Issues](https://github.com/PanzerPeter/Neuro/issues).

## Performance Issues

### Slow Compilation

**Symptoms**: compiling takes longer than expected.

**Causes**: a debug build of `neurc`, a large program, or a constrained machine.

**Solution**: run the compiler from a release build:

```bash
cargo build --release -p neurc
target/release/neurc compile program.nr
```

## Development Environment Issues

### VSCode Syntax Highlighting Not Working

**Symptoms**: `.nr` files show no syntax highlighting.

**Solution**: open the Extensions view, check that **Neuro Language Support** is installed and enabled, then run `Developer: Reload Window`. See [Editor Support](editor-support.md#vs-code). An editor that was already open does not pick up a new grammar on its own.

### Git Shows Every File as Modified (Windows)

**Symptoms**: `git status` lists all files as changed after cloning.

**Solution**: `git config --global core.autocrlf true`.

## Debugging Techniques

### Enable Debug Logging

`RUST_LOG` makes `neurc` log each pipeline stage to stderr. The levels are listed under [RUST_LOG](cli-usage.md#rust_log).

```bash
RUST_LOG=debug neurc compile program.nr 2> debug.log    # Unix
$env:RUST_LOG="debug"; neurc compile program.nr 2> debug.log   # PowerShell
```

### Check Each Stage

Narrow the failure to a stage: `neurc check` runs the front end only, `neurc compile` adds code generation and linking, and running the output tests the program itself.

```bash
neurc check program.nr
neurc compile program.nr
./program
```

## Common Warnings

The compiler emits two lint warnings. Warnings never block compilation. Each names the lint, so you can silence it with `@allow(...)` on the enclosing function, and points at the code the way an error does.

### `prefer_loop_over_while_true`

```text
warning[prefer_loop_over_while_true]: `while true { ... }` should be written as `loop { ... }`; silence with `@allow(prefer_loop_over_while_true)` on the enclosing function
 --> main.nr:2:11
  |
2 |     while true {
  |           ^^^^
```

A `while` whose condition is the literal `true`. `loop { ... }` says the same thing:

```neuro
while true {  // warning: prefer loop over while true
    break
}

loop {
    break
}
```

### `float_cast_out_of_range`

```text
warning[float_cast_out_of_range]: constant `1e20` does not fit `i32` and saturates to its maximum; use `.to_checked::<i32>()` for an `Option`, or silence with `@allow(float_cast_out_of_range)` on the enclosing function or `const`
 --> main.nr:2:13
  |
2 |     val x = 1e20 as i32
  |             ^^^^^^^^^^^
```

An `as` cast from a float to an integer, where the operand is a constant the target cannot hold, or NaN. The cast clamps the value (NaN becomes `0`), which is rarely what was meant. Fix the constant or widen the target. For a value known only at run time, `.to_checked::<T>()` returns `None` instead of clamping. If the clamp is intended, add `@allow(float_cast_out_of_range)` to the enclosing function or to the `const` holding the cast.

## Known Limitations

Features that are planned but not built yet are on the [Quick Roadmap](../../README.md#quick-roadmap). Each language reference page states the limits of the feature it covers. The one that surprises people most often: **an interpolation hole cannot contain a string literal.** `"{"lit"}"` is a lexical error; bind the string to a name first and interpolate the name.

## FAQ

### Why does my program compile but do nothing?

Check that `main` returns the expected exit code and performs the operations you expect. `neurc run` passes `main`'s return value through as its own exit status.

### Why can't I mix i32 and i64?

Neuro is strictly typed and has no implicit conversions. Convert explicitly with `as`: `val wide: i64 = narrow as i64`.

### Why is compilation slow?

Use `-O0` for the fastest compile in a debug loop, and `-O2` or `-O3` for faster binaries.

### How do I speed up development?

Use `neurc check` for feedback without code generation.

### Can I use Neuro for production?

Not yet. The language is alpha, and its syntax and semantics can still change between releases. The [Quick Roadmap](../../README.md#quick-roadmap) shows what is complete and what is ahead.

## Reporting Issues

Before reporting:
1. Check this page and the [CLI Usage Guide](cli-usage.md).
2. Search [GitHub Issues](https://github.com/PanzerPeter/Neuro/issues).
3. Capture a debug log with `RUST_LOG=debug neurc compile program.nr 2> debug.log`.
4. Reduce the program to a minimal file that still fails.

Include in the report, following [CONTRIBUTING.md](../../CONTRIBUTING.md):

```markdown
## Environment
- OS: Windows 11 / Ubuntu 22.04 / macOS 13
- Neuro: output of `neurc --version`, and the commit hash
- LLVM: 23.x
- Rust: 1.98.1+

## Issue
[Description]

## Reproduction
[Minimal .nr file that reproduces the issue]

## Expected
[What should happen]

## Actual
[What actually happens]

## Error Output
[Complete error message, with the debug log attached]
```

## See also

- [CLI Usage](cli-usage.md)
- [Installation](../getting-started/installation.md)
- [Editor Support](editor-support.md)
- [Language Reference](../language-reference/types.md)
- [Contributing](../../CONTRIBUTING.md)
