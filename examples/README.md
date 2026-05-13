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
Floating-point arithmetic operations and comparisons.

**Features:**
- f64 floating-point type
- Float arithmetic (+, -, *, /)
- Function parameters with float types
- IEEE-754 ordered float comparisons (`<, >, <=, >=`)
- NaN handling semantics returning false on inequalities

**Compile and run:**
```bash
cargo run -p neurc -- compile examples/float_ops.nr
.\examples\float_ops.exe
```

**Expected Exit Code:** `42`

### [control_flow.nr](control_flow.nr)
Control flow patterns with if/else statements.

**Features:**
- Boolean comparisons
- Multiple if statements
- While loops
- Range-for loops (`for i in start..end`)
- Function composition

**Compile and run:**
```bash
cargo run -p neurc -- compile examples/control_flow.nr
.\examples\control_flow.exe
```

### [for_range.nr](for_range.nr)
Range-for loop iteration with exclusive upper bound.

**Features:**
- `for i in start..end` syntax
- Integer range accumulation
- Exclusive range upper bound semantics

**Compile and run:**
```bash
cargo run -p neurc -- compile examples/for_range.nr
./examples/for_range
# Exit code: 10
```

### [for_range_inclusive.nr](for_range_inclusive.nr)
Range-for loop iteration with an inclusive upper bound.

**Features:**
- `for i in start..=end` syntax
- Integer range accumulation
- Inclusive range upper bound semantics

**Compile and run:**
```bash
cargo run -p neurc -- compile examples/for_range_inclusive.nr
./examples/for_range_inclusive
# Exit code: 15
```

### [constants.nr](constants.nr)
Compile-time constants at module and function scope.

**Features:**
- `const NAME: Type = expr` at module scope
- `const` inside function bodies
- Constant arithmetic (references between consts)
- Forward references (function uses const defined later in file)

**Compile and run:**
```bash
cargo run -p neurc -- compile examples/constants.nr
./examples/constants
# Exit code: 51
```

### [bitwise_ops.nr](bitwise_ops.nr)
Bitwise flag manipulation using `&`, `|`, `^`, `~`, and `<<`.

**Features:**
- Left shift (`<<`) to define bit-flag constants
- Bitwise OR (`|`) to set flags
- Bitwise AND (`&`) to test flags
- Bitwise XOR (`^`) to toggle flags
- Integer-type requirement (floats/bools rejected)

**Compile and run:**
```bash
cargo run -p neurc -- compile examples/bitwise_ops.nr
./examples/bitwise_ops
# Exit code: 1  (READ flag set; WRITE toggled off; EXECUTE never set)
```

### [integer_suffixes.nr](integer_suffixes.nr)
Integer literal type suffixes (`42i64`, `255u8`, `0xFFu8`, `0b1010i32`).

**Features:**
- All eight suffix variants: `i8`, `i16`, `i32`, `i64`, `u8`, `u16`, `u32`, `u64`
- Suffix pins the type without an explicit annotation (`val x = 42i64` infers `i64`)
- Works with decimal, hex, binary, and octal literals
- Range violations rejected at compile time (`300u8` is a compile error)

**Compile and run:**
```bash
cargo run -p neurc -- compile examples/integer_suffixes.nr
./examples/integer_suffixes
# Exit code: 0
```

### [if_block_expressions.nr](if_block_expressions.nr)
`if`/`else` chains and bare block expressions used as first-class values.

**Features:**
- `if`/`else` as a value: `val abs = if n >= 0 { n } else { 0 - n }`
- Chained `else if` as a value: `val clamped = if n < lo { lo } else if n > hi { hi } else { n }`
- Bare block expression as a value: `val area = { val w: i32 = 6; val h: i32 = 7; w * h }`
- All arms type-checked to produce the same type

**Compile and run:**
```bash
cargo run -p neurc -- compile examples/if_block_expressions.nr
./examples/if_block_expressions
# Exit code: 149  (abs(−7)=7 + area=42 + clamp(150,0,100)=100)
```

## Known Limitations

- No arrays yet (Phase 2+)
- Ownership/borrow checker not yet implemented (Phase 1.5)
- Right shift is `.shr(n)` method, not `>>` operator (Phase 2+)

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
- integer_suffixes.nr returns 0 (sum_bytes(10u8, 20u8) - 30)

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
