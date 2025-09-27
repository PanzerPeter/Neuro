# 01_basics - Fundamental NEURO Concepts

This directory contains examples demonstrating the fundamental building blocks of the NEURO programming language. These examples are designed for complete beginners and cover the essential concepts needed to write basic NEURO programs.

## Learning Path

Work through these examples in order for the best learning experience:

### 1. [01_hello_world.nr](./01_hello_world.nr)
- Your first NEURO program
- Program structure and entry point
- Basic output with print()
- Return values and exit codes

### 2. [02_comments.nr](./02_comments.nr)
- Line comments with `//`
- Block comments with `/* */`
- Nested comments
- Documentation best practices

### 3. [03_literals.nr](./03_literals.nr)
- Integer literals (positive, negative, zero)
- Float literals (decimal notation)
- String literals and escape sequences
- Boolean literals (true/false)

### 4. [04_variables.nr](./04_variables.nr)
- Variable declarations with `let`
- Mutable variables with `let mut`
- Type inference vs explicit annotations
- Variable scoping and shadowing

### 5. [05_operators.nr](./05_operators.nr)
- Arithmetic operators (`+`, `-`, `*`, `/`, `%`)
- Comparison operators (`<`, `<=`, `>`, `>=`, `==`, `!=`)
- Logical operators (`&&`, `||`, `!`)
- Operator precedence and parentheses

## Key Concepts Covered

- **Program Structure**: Every NEURO program starts with a `main()` function
- **Output**: Use `print()` to display values
- **Variables**: Immutable by default, use `mut` for mutability
- **Types**: Strong static typing with intelligent inference
- **Comments**: Essential for code documentation and explanation
- **Expressions**: Rich expression system with proper precedence

## Running the Examples

Each example can be run independently:

```bash
# Run a specific example
neurc run examples/01_basics/01_hello_world.nr

# Or compile and run
neurc build examples/01_basics/01_hello_world.nr
./01_hello_world
```

## What's Next?

After mastering these basics, continue to:
- **02_functions**: Function definitions, parameters, and scope
- **03_control_flow**: Conditional statements and loops
- **04_types**: Advanced type system features

## Implementation Notes

These examples use only fully implemented NEURO features:
- ✅ All syntax shown is fully supported
- ✅ All examples compile and run with current `neurc`
- ✅ Examples focus on core language mechanics

Some advanced features are mentioned but not demonstrated where they're not yet fully implemented in the compiler.