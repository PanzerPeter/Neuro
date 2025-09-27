# 02_functions - Functions and Code Organization

This directory contains examples demonstrating function definition, calling, and organization in NEURO. Functions are the primary way to structure and reuse code in NEURO programs.

## Learning Path

Work through these examples in order:

### 1. [01_basic_functions.nr](./01_basic_functions.nr)
- Function definition syntax (`fn name() -> type`)
- Functions with and without parameters
- Functions with and without return values
- Void functions and return statements
- Basic function calling

### 2. [02_parameters_return.nr](./02_parameters_return.nr)
- Single and multiple parameter patterns
- Type annotations for parameters
- Different return types (`int`, `bool`)
- Parameter usage and reuse within functions
- Complex parameter combinations

### 3. [03_recursion.nr](./03_recursion.nr)
- Recursive function patterns
- Base cases and recursive cases
- Classic algorithms (factorial, fibonacci, GCD)
- Tail recursion optimization patterns
- Recursion depth considerations

### 4. [04_scope_examples.nr](./04_scope_examples.nr)
- Function parameter scope
- Local variable scope
- Variable shadowing within functions
- Block scope interactions
- Parameter immutability

## Key Concepts Covered

### Function Basics
- **Syntax**: `fn name(params) -> return_type { body }`
- **Parameters**: Typed, immutable within function
- **Return Types**: Explicit type annotations
- **Void Functions**: No return type specified

### Function Features
- **Recursion**: Functions can call themselves
- **Nesting**: Functions can call other functions
- **Scope**: Lexical scoping with shadowing support
- **Type Safety**: All parameters and returns are type-checked

### Advanced Patterns
- **Multiple Return Paths**: Early returns with conditionals
- **Parameter Patterns**: Single, multiple, and complex parameter usage
- **Recursive Algorithms**: Mathematical and algorithmic patterns
- **Scope Management**: Understanding variable visibility

## Function Signature Patterns

```neuro
// No parameters, returns int
fn get_value() -> int { return 42; }

// Single parameter, returns calculation
fn square(n: int) -> int { return n * n; }

// Multiple parameters, returns result
fn add(a: int, b: int) -> int { return a + b; }

// Void function (no return type)
fn print_message() { print(42); return; }

// Boolean return type
fn is_positive(n: int) -> bool { return n > 0; }
```

## Running the Examples

```bash
# Run individual examples
neurc run examples/02_functions/01_basic_functions.nr
neurc run examples/02_functions/02_parameters_return.nr
neurc run examples/02_functions/03_recursion.nr
neurc run examples/02_functions/04_scope_examples.nr

# Or compile and run
neurc build examples/02_functions/01_basic_functions.nr
./01_basic_functions
```

## Common Patterns Demonstrated

### Mathematical Functions
- Arithmetic operations
- Absolute value and sign functions
- Power and factorial calculations
- Greatest common divisor

### Conditional Logic in Functions
- Multiple return paths
- Early returns with guards
- Parameter validation patterns

### Recursive Patterns
- Single recursion (factorial)
- Multiple recursion (fibonacci)
- Tail recursion optimization
- Helper function patterns

## What's Next?

After mastering functions, continue to:
- **03_control_flow**: Conditional statements and loops
- **04_types**: Advanced type system features
- **05_data_structures**: Organizing data with structs

## Implementation Notes

All examples use fully implemented NEURO features:
- ✅ Function definitions and calls work completely
- ✅ Recursion is fully supported
- ✅ Parameter passing and return values work correctly
- ✅ Scope rules are properly implemented
- ✅ Type checking for functions is complete

These examples demonstrate real, working NEURO code that compiles and runs with the current `neurc` implementation.