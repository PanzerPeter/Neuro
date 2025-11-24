# Troubleshooting Guide

Common problems and solutions when working with NEURO.

## Installation Issues

### "No suitable version of LLVM was found"

**Symptoms**:
```
error: No suitable version of LLVM was found system-wide or pointed
       to by LLVM_SYS_181_PREFIX.
```

**Cause**: LLVM_SYS_181_PREFIX not set or points to wrong location.

**Solution**:

**Windows**:
```powershell
# Set environment variable
[System.Environment]::SetEnvironmentVariable('LLVM_SYS_181_PREFIX', 'C:\LLVM-1818', 'Machine')

# Restart terminal and verify
echo %LLVM_SYS_181_PREFIX%
```

**Unix**:
```bash
# Add to ~/.bashrc or ~/.zshrc
export LLVM_SYS_181_PREFIX=/usr/lib/llvm-18

# Reload shell config
source ~/.bashrc

# Verify
echo $LLVM_SYS_181_PREFIX
```

### "LLVMConfig.cmake not found"

**Symptoms**:
```
Could not find LLVMConfig.cmake
```

**Cause**: Installed .exe installer instead of full development package.

**Solution**:

Download and extract the full development package:
- Windows: `clang+llvm-18.1.8-x86_64-pc-windows-msvc.tar.xz`
- URL: https://github.com/llvm/llvm-project/releases/tag/llvmorg-18.1.8

**DO NOT** use the `.exe` installer - it lacks required development files.

### "cannot open input file 'libxml2s.lib'" (Windows)

**Symptoms**:
```
LINK : fatal error LNK1181: cannot open input file 'libxml2s.lib'
```

**Cause**: libxml2 not installed via vcpkg.

**Solution**:
```powershell
cd C:\vcpkg
.\vcpkg install libxml2:x64-windows-static
.\vcpkg integrate install

# Verify installation
.\vcpkg list | findstr libxml2
```

### Build fails with linker errors (Unix)

**Symptoms**:
```
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

## Compilation Errors

### Type Mismatch Errors

**Symptoms**:
```
Type error: Type mismatch
  expected: i32
  found: f64
  at program.nr:5:18
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
```
Type error: Undefined variable 'x'
  at program.nr:8:12
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
```
Type error: Cannot assign to immutable variable 'x'
  at program.nr:6:5
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
```
Type error: Missing return statement
  expected return type: i32
  at program.nr:5:1
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
```
Parse error: unexpected token `}`, expected expression
  at program.nr:10:5
```

**Common causes**:
1. Missing semicolons on statements
2. Unbalanced brackets/braces
3. Syntax errors

**Solutions**:

**Check semicolons**:
```neuro
// Error: missing semicolon
val x: i32 = 10
val y: i32 = 20

// Fix: add semicolons for statements
val x: i32 = 10  // Semicolon required
val y: i32 = 20  // Semicolon required
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
- Add exception for NEURO executables
- Temporarily disable to test

### Permission Denied (Unix)

**Symptoms**:
```
bash: ./program: Permission denied
```

**Cause**: Executable permission not set.

**Solution**:
```bash
chmod +x ./program
./program
```

### Segmentation Fault

**Symptoms**:
Program crashes with segmentation fault.

**Causes** (rare in Phase 1):
1. Integer overflow
2. Division by zero
3. Compiler bug

**Solutions**:

**Check for division by zero**:
```neuro
// Potential crash
val x: i32 = 10 / 0  // Division by zero

// Fix: check denominator
func safe_divide(a: i32, b: i32) -> i32 {
    if b == 0 {
        return 0  // Or handle error
    } else {
        return a / b
    }
}
```

**Report compiler bugs**:
- GitHub Issues: https://github.com/PanzerPeter/Neuro/issues
- Include minimal reproduction case

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

**Check system resources**:
- Close unnecessary applications
- Ensure sufficient RAM (minimum 4GB recommended)
- Check disk space

### Large Executable Size

**Symptoms**:
Executable is larger than expected.

**Causes**:
1. Debug information included
2. No optimization (Phase 1 limitation)

**Solutions**:

**Current** (Phase 1):
- Executable size is unoptimized
- Typical size: 1-5 MB for simple programs

**Future** (Phase 1.5+):
- Optimization levels will reduce size
- Strip debug info with linker options

## Development Environment Issues

### VSCode Syntax Highlighting Not Working

**Symptoms**:
`.nr` files show no syntax highlighting.

**Solution**:

Install NEURO VSCode extension:
```bash
cd neuro-language-support
npm install -g @vscode/vsce
vsce package
# Install .vsix file in VSCode
```

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
5. Check CLAUDE.md for development guidelines

### Reporting Issues

Include in bug reports:
- NEURO compiler version
- Operating system and version
- LLVM version
- Rust version
- Complete error message
- Minimal reproduction case
- Steps to reproduce

**Template**:
```markdown
## Environment
- OS: Windows 11 / Ubuntu 22.04 / macOS 13
- NEURO: Phase 1 (commit hash)
- LLVM: 18.1.8
- Rust: 1.70+

## Issue
[Description]

## Reproduction
[Minimal .nr file that reproduces the issue]

## Expected
[What should happen]

## Actual
[What actually happens]

## Error Output
```
[Complete error message with debug logging]
```
```

### Resources

- GitHub Issues: https://github.com/PanzerPeter/Neuro/issues
- Documentation: [README.md](../../README.md)
- Development Guidelines: [CLAUDE.md](../../CLAUDE.md)
- Architecture: [VSA_Rust_3_0.xml](../../VSA_Rust_3_0.xml)

## Known Limitations (Phase 1)

These are not bugs, but current limitations:

1. **Type inference**: Numeric literals default to i32/f64
2. **String type**: Basic support pending
3. **Loops**: Not yet implemented (Phase 2)
4. **Arrays**: Not yet implemented (Phase 2)
5. **Optimization**: Only -O0 supported

See [Roadmap](../../.idea/roadmap.md) for planned features.

## Common Warnings

### Unreachable Code

**Message**:
```
warning: unreachable code
  at program.nr:8:5
```

**Cause**: Code after return statement.

**Solution**:
```neuro
// Warning: unreachable
func example() -> i32 {
    return 42
    val x: i32 = 10  // Warning: unreachable
}

// Fix: remove or move code
func example() -> i32 {
    val x: i32 = 10  // Execute before return
    return 42
}
```

## FAQ

### Why does my program compile but do nothing?

Check that `main` returns the expected exit code and performs desired operations.

### Why can't I mix i32 and i64?

NEURO uses strict typing with no implicit conversions. Explicit conversion operators coming in Phase 2.

### Why is compilation slow?

Phase 1 uses `-O0` (no optimization). This prioritizes compilation speed over execution speed. Optimization levels coming in Phase 1.5+.

### How do I speed up development?

Use `neurc check` for rapid feedback without code generation.

### Can I use NEURO for production?

Not yet - Phase 1 is alpha stage. Wait for Phase 2+ for production readiness.

## Still Stuck?

If this guide doesn't solve your problem:

1. Check [CLI Usage Guide](cli-usage.md)
2. Review [Language Reference](../language-reference/types.md)
3. Search or create [GitHub Issue](https://github.com/PanzerPeter/Neuro/issues)
4. Include all requested information in bug reports
