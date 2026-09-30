# Troubleshooting Guide

Common problems and solutions when working with Neuro.

## Installation Issues

### "No suitable version of LLVM was found"

**Symptoms**:
```text
error: No suitable version of LLVM was found system-wide or pointed
       to by LLVM_SYS_221_PREFIX.
```

**Cause**: LLVM_SYS_221_PREFIX not set or points to wrong location. Neuro requires LLVM 22.

**Solution**:

**Windows**:
```powershell
# Set environment variable (adjust path to your LLVM 22 install)
[System.Environment]::SetEnvironmentVariable('LLVM_SYS_221_PREFIX', 'C:\LLVM', 'Machine')

# Restart terminal and verify
$env:LLVM_SYS_221_PREFIX
```

**Unix**:
```bash
# Add to ~/.bashrc or ~/.zshrc (path varies by distro)
export LLVM_SYS_221_PREFIX=/usr/lib/llvm-22   # Ubuntu/Debian
# export LLVM_SYS_221_PREFIX=/usr             # Arch/CachyOS

# Reload shell config
source ~/.bashrc

# Verify
echo $LLVM_SYS_221_PREFIX
```

### "LLVMConfig.cmake not found"

**Symptoms**:
```text
Could not find LLVMConfig.cmake
```

**Cause**: Installed .exe installer instead of full development package.

**Solution**:

Download and extract the full development package:
- Windows: the LLVM 22 `clang+llvm-22.*-x86_64-pc-windows-msvc.tar.xz` archive
- URL: https://github.com/llvm/llvm-project/releases

**Do not** use the `.exe` installer: it lacks the development files.

### "cannot open input file 'xml2s.lib'" (Windows)

**Symptoms**:
```text
LINK : fatal error LNK1181: cannot open input file 'xml2s.lib'
```

**Cause**: LLVM's `llvm-config.exe --system-libs` names `xml2s.lib`, because LLVM's
Windows manifest merger uses libxml2. Nothing Neuro calls reaches it, so no libxml2 code
is linked, but the linker still stops when a named library is missing.

**Solution**: copy any static libxml2 into the LLVM `lib` directory under that name:
```powershell
vcpkg install "libxml2[core]:x64-windows-static"
Get-ChildItem "<vcpkg-root>\installed\x64-windows-static\lib\*xml2*.lib" |
  Select-Object -First 1 | Copy-Item -Destination "$env:LLVM_SYS_221_PREFIX\lib\xml2s.lib"
```

### Build fails with linker errors (Unix)

**Symptoms**:
```text
error: linker `cc` not found
```

**Cause**: Missing C/C++ compiler toolchain.

**Solution**:

**Ubuntu/Debian**:
```bash
sudo apt-get update
sudo apt-get install build-essential
```

**Arch**:
```bash
sudo pacman -S base-devel
```

**macOS**:
```bash
xcode-select --install
```

### `cargo build --features mlir` fails

The `mlir-backend` slice's `mlir` feature is opt-in and needs MLIR 22 on top of LLVM 22.
Default builds compile a placeholder and need none of this.

**Symptoms**:
```text
failed to run `".../bin/llvm-config" ...`
# or
fatal error: 'mlir-c/IR.h' file not found
# or, at link time
unable to find library -lMLIR-C
```

**Cause**: `mlir-sys` finds MLIR by running `$MLIR_SYS_220_PREFIX/bin/llvm-config` and
reading its `lib/` and `include/`. MLIR has to live in the same prefix as that
`llvm-config`, and a prefix with no static LLVM libraries makes `mlir-sys` link the shared
`MLIR-C` library, which distro packages (Arch's `aur/mlir` included) do not build.

**Solution**: use a prefix that holds LLVM and MLIR together: apt.llvm.org's
`/usr/lib/llvm-22` with `libmlir-22-dev` installed, or a source build of LLVM 22 with
`-DLLVM_ENABLE_PROJECTS=mlir`. Point all three variables at it. See
[Installation → Optional: MLIR Backend](../getting-started/installation.md#optional-mlir-backend).

```bash
export LLVM_SYS_221_PREFIX=<prefix>   # inkwell
export MLIR_SYS_220_PREFIX=<prefix>   # melior
export TABLEGEN_220_PREFIX=<prefix>   # melior's TableGen macros
cargo test -p mlir-backend --features mlir
```

If bindgen reports opaque 1-byte structs (`attempt to compute 0_usize - 8_usize`), the
libclang it loaded is a different major version from the MLIR headers. Point
`LIBCLANG_PATH` at a libclang 22.

## Compilation Errors

### Type Mismatch Errors

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

**Causes**:
1. Incorrect type in assignment or return
2. Mixing integer and float types
3. Using wrong type for function argument

**Solutions**:

**Check variable types**:
```neuro
// Error: mixing types
val x: i32 = 10
val y: f64 = 3.14
// val z = x + y  // Type mismatch

// Fix: use same types
val x: f64 = 10.0
val y: f64 = 3.14
val z: f64 = x + y  // OK
```

**Check function signatures**:
```neuro
func takes_i32(x: i32) -> i32 {
    return x
}

// Error: passing wrong type
val y: f64 = 3.14
// takes_i32(y)  // Type mismatch

// Fix: use correct type
val x: i32 = 42
takes_i32(x)  // OK
```

### Undefined Variable Errors

**Symptoms**:
```text
Type errors found in "program.nr":
error: undefined variable 'z'
 --> program.nr:2:14
  |
2 |     return z + 1
  |            ^

Error: 1 type error(s) found
```

**Causes**:
1. Variable not declared before use
2. Typo in variable name
3. Variable out of scope

**Solutions**:

**Declare before use**:
```neuro
// Error: undefined
// return x

// Fix: declare first
val x: i32 = 42
return x
```

**Check scope**:
```neuro
func scoped() -> i32 {
    if true {
        val x: i32 = 10
    }
    // return x  // Error: x out of scope

    // Fix: declare in correct scope
    val x: i32 = 10
    if true {
        // x is accessible here
    }
    return x  // OK
}
```

### Cannot Assign to Immutable Variable

**Symptoms**:
```text
Type errors found in "program.nr":
error: cannot assign to immutable variable 'x'
 --> program.nr:3:5
  |
3 |     x = 20
  |     ^^^^^^

Error: 1 type error(s) found
```

**Cause**: Trying to reassign `val` variable.

**Solution**:

Use `mut` for variables that need to change:

```neuro
// Error: immutable
val x: i32 = 10
// x = 20  // Error

// Fix: use mut
mut y: i32 = 10
y = 20  // OK
```

### Missing Return Statement

**Symptoms**:
```text
Type errors found in "program.nr":
error: missing return statement in function returning i32
 --> program.nr:1:1
  |
1 | func compute(x: i32) -> i32 {
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^

Error: 1 type error(s) found
```

**Cause**: Function doesn't return a value on all code paths.

**Solutions**:

**Add missing return**:
```neuro
// Error: missing return for x <= 0
func bad(x: i32) -> i32 {
    if x > 0 {
        return x
    }
    // Missing return for else case
}

// Fix: add else branch
func good(x: i32) -> i32 {
    if x > 0 {
        return x
    } else {
        return 0
    }
}
```

**Use implicit return**:
```neuro
func good_implicit(x: i32) -> i32 {
    if x > 0 {
        x
    } else {
        0
    }
}
```

### Parse Errors

**Symptoms**:
```text
error: unexpected token RightBrace, expected expression
 --> program.nr:3:1
  |
3 | }
  | ^

Error: Parsing failed
```

Parse errors stop the run at the first one; only the type checker reports a list.

**Common causes**:
1. A stray semicolon (Neuro has none, see below)
2. Unbalanced brackets/braces
3. Syntax errors

**Solutions**:

**Remove semicolons**: Neuro statements are terminated by a newline, not `;`.
A trailing semicolon is an `unexpected token Semicolon` parse error:
```neuro
// Error: semicolons are not valid tokens
val x: i32 = 10;
val y: i32 = 20;

// Fix: one statement per line, no `;`
val x: i32 = 10
val y: i32 = 20
```

**Check brackets**:
```neuro
// Error: unbalanced braces
func bad() -> i32 {
    if true {
        return 1
    // Missing closing brace
}

// Fix: add missing brace
func good() -> i32 {
    if true {
        return 1
    }  // Closing brace added
}
```

## Runtime Issues

### Executable Doesn't Run (Windows)

**Symptoms**:
- Executable created but won't run
- "Cannot find DLL" errors

**Causes**:
1. Missing MSVC runtime
2. Antivirus blocking execution

**Solutions**:

**Install Visual C++ Redistributable**:
- Download from: https://aka.ms/vs/17/release/vc_redist.x64.exe
- Install and restart

**Check antivirus**:
- Add exception for Neuro executables
- Temporarily disable to test

### Permission Denied (Unix)

**Symptoms**:
```text
bash: ./program: Permission denied
```

**Cause**: Executable permission not set.

**Solution**:
```bash
chmod +x ./program
./program
```

### Runtime Panic

**Symptoms**:
The program prints a `panic:` line naming a source position, then aborts (`SIGABRT`, which a
shell reports as status 134; `neurc run` reports it as status `1`):

```text
panic: division by zero at program.nr:1:33
```

**Causes**:
1. Integer division or remainder by zero. The divisor is checked at every optimization level.
2. Integer overflow in a build at `-O0`, reported as `panic: integer overflow`. From `-O1`
   up, arithmetic wraps silently.
3. An explicit `panic(msg)`, a failed `assert(cond)`, or a reached `unreachable()`.

**Solutions**:

**Check the divisor**:
```neuro
func safe_divide(a: i32, b: i32) -> i32 {
    if b == 0 {
        return 0  // or report the failure through an Option or Result
    }
    return a / b
}
```

For overflow, `.checked_add`, `.checked_sub` and `.checked_mul` return an `Option` instead
of panicking; see [integer methods](../language-reference/types.md#integer-methods).

### Runtime Crash (signal)

A program that dies on a signal (`SIGSEGV`, `SIGILL`) rather than a `panic:` line has hit
either unbounded recursion or a compiler bug. Report it with a minimal reproduction:
- GitHub Issues: https://github.com/PanzerPeter/Neuro/issues

## Performance Issues

### Slow Compilation

**Symptoms**:
Compilation takes longer than expected.

**Causes**:
1. Debug build of compiler
2. Large program
3. System resource constraints

**Solutions**:

**Use release build**:
```bash
# Build compiler in release mode
cargo build --release -p neurc

# Use release build
cargo run --release -p neurc -- compile program.nr
```

## Development Environment Issues

### VSCode Syntax Highlighting Not Working

**Symptoms**:
`.nr` files show no syntax highlighting.

**Solution**:

Install the extension from `neuro-language-support/` as described in
[Editor Support](editor-support.md#vs-code), then reload the window
(`Developer: Reload Window`). An editor that was already open does not pick up a new
grammar on its own.

### Git Line Ending Issues (Windows)

**Symptoms**:
Git shows all files as modified.

**Solution**:
```bash
# Configure Git for cross-platform development
git config --global core.autocrlf true
```

## Debugging Techniques

### Enable Debug Logging

Get detailed compilation information:

```bash
# Windows (PowerShell)
$env:RUST_LOG="debug"
neurc compile program.nr 2> debug.log

# Unix
RUST_LOG=debug neurc compile program.nr 2> debug.log

# Review log
cat debug.log
```

### Isolate the Problem

Create minimal reproduction:

```neuro
// Start with simplest program
func main() -> i32 {
    return 0
}

// Gradually add code until error appears
```

### Check Each Stage

Test compilation stages separately:

```bash
# 1. Check syntax only
neurc check program.nr

# 2. If check passes, try compile
neurc compile program.nr

# 3. If compile passes, try run
./program
```

## Getting Help

### Before Asking for Help

1. Check this troubleshooting guide
2. Search GitHub issues
3. Enable debug logging
4. Create minimal reproduction
5. Review CONTRIBUTING.md for development and reporting guidelines

### Reporting Issues

Include in bug reports:
- Neuro compiler version
- Operating system and version
- LLVM version
- Rust version
- Complete error message
- Minimal reproduction case
- Steps to reproduce

**Template**:
````markdown
## Environment
- OS: Windows 11 / Ubuntu 22.04 / macOS 13
- Neuro: version from `neurc --version` (and commit hash)
- LLVM: 22.x
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
```text
[Complete error message with debug logging]
```
````

### Resources

- GitHub Issues: https://github.com/PanzerPeter/Neuro/issues
- Documentation: [README.md](../../README.md)
- Development Guidelines: [CONTRIBUTING.md](../../CONTRIBUTING.md)

## Known Limitations

Features that are planned but not built yet are on the
[Quick Roadmap](../../README.md#quick-roadmap). Each language reference page states the
limits of the feature it covers. Two that surprise people most often:

1. **A generic cannot be instantiated with an enclosing type parameter.** Inside
   `func f<T>`, a type such as `Option<T>` is rejected with "nested generic type argument
   is not yet supported". Concrete arguments (`Option<i32>`, `Box<string>`) work.
2. **An interpolation hole cannot contain a string literal.** `"{"lit"}"` is a lexical
   error; bind the string to a name first and interpolate the name.

## Common Warnings

The compiler currently emits two warnings. Warnings never block compilation.

### `prefer-loop-over-while-true`

**Message**:
```text
warning[prefer-loop-over-while-true] at 24..28: `while true { ... }` should be written as `loop { ... }`; silence with `@allow(prefer_loop_over_while_true)` on the enclosing function
```

**Cause**: A `while` loop whose condition is the literal `true`. `loop { ... }` says the same
thing and is the idiomatic form.

**Solution**:
```neuro
// Triggers the lint
while true {
    break
}

// Preferred
loop {
    break
}
```

Silence it with `@allow(prefer_loop_over_while_true)` on the enclosing function when the literal
form reads better.

### `float-cast-out-of-range`

**Message**:
```text
warning[float-cast-out-of-range] at 42..53: constant `1e20` does not fit `i32` and saturates to its maximum; use `.to_checked::<i32>()` for an `Option`, or silence with `@allow(float_cast_out_of_range)` on the enclosing function
```

**Cause**: An `as` cast from a float to an integer type whose operand is a constant the target
cannot hold, or NaN. The cast clamps it to the type's minimum or maximum (NaN becomes `0`),
which is rarely the value the author meant.

**Solution**: Fix the constant or widen the target type. Where the program computes the value
at run time, `.to_checked::<T>()` returns `None` for it instead of clamping. If the clamp is
what you want, add `@allow(float_cast_out_of_range)` to the enclosing function or method.

## FAQ

### Why does my program compile but do nothing?

Check that `main` returns the expected exit code and performs desired operations.

### Why can't I mix i32 and i64?

Neuro uses strict typing with no implicit conversions. Convert explicitly with an `as`
cast: `val wide: i64 = narrow as i64`.

### Why is compilation slow?

Use `-O0` for fastest compile/debug loops and `-O2`/`-O3` for faster runtime binaries.

### How do I speed up development?

Use `neurc check` for rapid feedback without code generation.

### Can I use Neuro for production?

Not yet. The language is alpha, and its syntax and semantics can still change between
releases. The [Quick Roadmap](../../README.md#quick-roadmap) shows what is complete and what
is ahead.

## Still Stuck?

If this guide doesn't solve your problem:

1. Check [CLI Usage Guide](cli-usage.md)
2. Review [Language Reference](../language-reference/types.md)
3. Search or create [GitHub Issue](https://github.com/PanzerPeter/Neuro/issues)
4. Include all requested information in bug reports
