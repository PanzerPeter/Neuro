# NEURO Example Programs

This directory contains example programs demonstrating Phase 1 features of the NEURO compiler.

## Available Examples

### [hello.nr](hello.nr)
Basic hello world program demonstrating function definitions, variables, and return statements.

**Features:**
- Function definitions
- Variable declarations
- Arithmetic operations
- Return statements

**Compile and run:**
```bash
cargo run -p neurc -- compile examples/hello.nr
.\examples\hello.exe
```

### [milestone.nr](milestone.nr)
Phase 1 milestone program showing core functionality.

**Features:**
- Function parameters
- Function calls
- Return values

**Compile and run:**
```bash
cargo run -p neurc -- compile examples/milestone.nr
.\examples\milestone.exe
```

### [factorial.nr](factorial.nr)
Recursive factorial calculation (5! = 120).

**Features:**
- Recursive function calls
- Conditional logic (if/else)
- Arithmetic operations

**Compile and run:**
```bash
cargo run -p neurc -- compile examples/factorial.nr
.\examples\factorial.exe
# Exit code: 120
```

### [fibonacci.nr](fibonacci.nr)
Recursive Fibonacci sequence calculation (fibonacci(10) = 55).

**Features:**
- Recursive functions
- Multiple recursion branches
- Variable assignments

**Compile and run:**
```bash
cargo run -p neurc -- compile examples/fibonacci.nr
.\examples\fibonacci.exe
# Exit code: 55
```

### [division.nr](division.nr)
Division and modulo operations.

**Features:**
- Division operator (/)
- Modulo operator (%)
- Function calls
- Combined arithmetic

**Compile and run:**
```bash
cargo run -p neurc -- compile examples/division.nr
.\examples\division.exe
```

### [float_ops.nr](float_ops.nr)
Floating-point arithmetic operations.

**Features:**
- f64 floating-point type
- Float arithmetic (+, -, *, /)
- Function parameters with float types

**Note:** Float comparisons in if conditions are not yet supported in Phase 1.

**Compile and run:**
```bash
cargo run -p neurc -- compile examples/float_ops.nr
.\examples\float_ops.exe
```

### [control_flow.nr](control_flow.nr)
Control flow patterns with if/else statements.

**Features:**
- Boolean comparisons
- Multiple if statements
- Function composition

**Note:** Phase 1 has limitations with deeply nested if/else chains.

**Compile and run:**
```bash
cargo run -p neurc -- compile examples/control_flow.nr
.\examples\control_flow.exe
```

## Phase 1 Limitations

While Phase 1 is feature-complete for its scope, there are known limitations:

1. **Float comparisons in if conditions:** The LLVM backend currently only handles integer comparisons in conditional expressions. Float arithmetic works, but using float values as boolean conditions will fail.

2. **Complex control flow:** Deeply nested if/else chains may not be recognized as having complete return coverage. Simple if/else patterns work reliably.

3. **No while/for loops:** Loop constructs are planned for Phase 2.

4. **No strings yet:** String type is defined but not fully functional.

5. **No arrays or structs:** These are Phase 2 features.

## Compiling Examples

All examples can be compiled using the `neurc` compiler:

```bash
# Check syntax and types only
cargo run -p neurc -- check examples/<filename>.nr

# Compile to executable
cargo run -p neurc -- compile examples/<filename>.nr

# Compile with custom output path
cargo run -p neurc -- compile examples/<filename>.nr -o my_program
```

## Exit Codes

Since NEURO programs return i32 from main(), the return value becomes the exit code of the executable. This is used in the examples and integration tests to verify correct computation.

For example:
- factorial.nr returns 120 (5!)
- fibonacci.nr returns 55 (fibonacci(10))
- milestone.nr returns 8 (5 + 3)

## Testing

All examples are tested in the integration test suite. Run tests with:

```bash
cargo test --workspace
```

## Contributing

When adding new examples:
1. Keep them simple and focused on demonstrating specific features
2. Add comments explaining what the example demonstrates
3. Document any Phase 1 limitations that affect the example
4. Test that they compile and run correctly
5. Add them to this README

## See Also

- [Language Reference](../docs/language-reference/types.md)
- [CHANGELOG](../CHANGELOG.md)
- [Compiler Documentation](../docs/README.md)
