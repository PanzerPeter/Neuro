# NEURO Programming Language Examples

This directory contains example programs demonstrating the features of the NEURO programming language.

## Running Examples

You can run any example using the NEURO compiler:

```bash
# Run directly with JIT execution
neurc run examples/01_basic_arithmetic.nr

# Or compile to executable first (requires LLVM tools)
neurc build examples/01_basic_arithmetic.nr
./01_basic_arithmetic
```

## Available Examples

### 01_basic_arithmetic.nr
Demonstrates basic arithmetic operations (addition, subtraction, multiplication, division).
- **Expected output**: 72
- **Features**: Variable declarations, arithmetic operations, return values

### 02_conditional_logic.nr
Shows conditional statements and comparison operations.
- **Expected output**: Variable depending on conditions
- **Features**: if/else statements, comparison operators, conditional logic

### 03_loops.nr
Demonstrates while loops and iterative calculations.
- **Expected output**: Sum calculation (1+2+3+4+5 = 15)
- **Features**: while loops, variable updates, iterative logic
- **Note**: Currently has infinite loop issue in JIT - being fixed

### 04_simple_example.nr
A simple working example demonstrating core functionality.
- **Expected output**: 40
- **Features**: Variables, multiplication, print function

## Language Features Demonstrated

- **Variables**: `let x = 5;`
- **Arithmetic**: `+`, `-`, `*`, `/`
- **Conditionals**: `if x > y { ... } else { ... }`
- **Loops**: `while condition { ... }`
- **Functions**: `fn main() -> int { ... }`
- **Print**: `print(value);`
- **Comments**: `// This is a comment`

## Current Implementation Status

✅ **Working Features**:
- Variable declarations and assignments
- Arithmetic operations
- Function definitions and returns
- Conditional statements (if/else)
- Print function
- JIT execution
- LLVM IR generation

🚧 **In Development**:
- Complex loop variable assignments
- Advanced control flow
- String literals
- More standard library functions

## Testing Your Own Programs

Create a new `.nr` file and run it:

```bash
# Create your program
echo 'fn main() -> int { let x = 42; print(x); return 0; }' > my_program.nr

# Run it
neurc run my_program.nr
```

The compiler supports both compilation to LLVM IR (when LLVM tools are available) and JIT execution (fallback when LLVM tools are not available).