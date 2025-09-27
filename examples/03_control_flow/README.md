# 03_control_flow - Conditional Logic and Loops

This directory contains examples demonstrating control flow constructs in NEURO, including conditional statements and iterative loops. These examples show how to control program execution flow based on conditions and create repetitive operations.

## Learning Path

Work through these examples in order for progressive learning:

### 1. [01_if_statements.nr](./01_if_statements.nr)
- Basic `if` statement syntax
- `if/else` and `else if` chains
- Nested if statements for complex decisions
- Complex boolean conditions with logical operators
- Early return patterns and guard clauses

### 2. [02_while_loops.nr](./02_while_loops.nr)
- Basic `while` loop syntax and conditions
- Loop counters and accumulator patterns
- Nested loops for multi-dimensional processing
- Complex loop termination conditions
- Common algorithmic patterns (factorial, fibonacci, etc.)

### 3. [03_break_continue.nr](./03_break_continue.nr)
- `break` statement for early loop termination
- `continue` statement for skipping iterations
- Break vs continue behavior differences
- Loop control in nested structures
- Search and filter patterns

### 4. [04_nested_control.nr](./04_nested_control.nr)
- Complex combinations of if/else and while loops
- Deep nesting patterns and their management
- Real-world algorithmic examples
- Performance considerations with nested structures
- Advanced control flow patterns

## Key Concepts Covered

### Conditional Statements
- **Basic if**: Single condition execution
- **if/else**: Binary decision making
- **else if chains**: Multiple condition sequences
- **Nested conditions**: Complex decision trees
- **Boolean logic**: AND (`&&`), OR (`||`), NOT (`!`)

### Loop Constructs
- **while loops**: Condition-based repetition
- **Loop variables**: Counters and accumulators
- **Nested loops**: Multi-dimensional iteration
- **Loop conditions**: Simple and complex termination rules

### Loop Control
- **break**: Immediate loop exit
- **continue**: Skip to next iteration
- **Early termination**: Efficient search patterns
- **Selective processing**: Conditional iteration

### Advanced Patterns
- **Guard clauses**: Early return patterns
- **Accumulator patterns**: Building results iteratively
- **Search algorithms**: Finding elements with early termination
- **Filter patterns**: Selective data processing

## Control Flow Syntax Summary

```neuro
// Basic if statement
if condition {
    // statements
}

// if/else statement
if condition {
    // true branch
} else {
    // false branch
}

// else if chain
if condition1 {
    // branch 1
} else if condition2 {
    // branch 2
} else {
    // default branch
}

// while loop
while condition {
    // loop body
}

// Loop control
while condition {
    if skip_condition {
        continue;  // Skip to next iteration
    }

    if exit_condition {
        break;     // Exit loop immediately
    }

    // normal processing
}
```

## Running the Examples

```bash
# Run individual examples
neurc run examples/03_control_flow/01_if_statements.nr
neurc run examples/03_control_flow/02_while_loops.nr
neurc run examples/03_control_flow/03_break_continue.nr
neurc run examples/03_control_flow/04_nested_control.nr

# Compile and run
neurc build examples/03_control_flow/01_if_statements.nr
./01_if_statements
```

## Algorithmic Patterns Demonstrated

### Mathematical Algorithms
- Factorial calculation (iterative and recursive approaches)
- Fibonacci sequence generation
- Greatest Common Divisor (GCD)
- Prime number checking
- Perfect square detection

### Search and Filter Patterns
- Linear search with early termination
- First occurrence finding
- Conditional filtering
- Range validation

### Data Processing Patterns
- Accumulation and summation
- Counting with conditions
- Matrix/grid processing
- Multi-pass algorithms

### Control Flow Optimization
- Early returns for efficiency
- Loop unrolling concepts
- Condition ordering for performance
- Break vs continue for readability

## Best Practices Demonstrated

### Readability
- Clear condition expressions
- Meaningful variable names
- Consistent indentation
- Logical flow organization

### Efficiency
- Early termination when possible
- Minimal condition evaluation
- Appropriate loop structure choice
- Guard clauses for edge cases

### Maintainability
- Avoiding deep nesting when possible
- Clear separation of concerns
- Consistent control flow patterns
- Well-commented complex logic

## What's Next?

After mastering control flow, continue to:
- **04_types**: Advanced type system features
- **05_data_structures**: Organizing data with structs
- **06_pattern_matching**: Pattern-based control flow

## Implementation Notes

All examples use fully implemented NEURO features:
- ✅ if/else/else if statements work completely
- ✅ while loops with complex conditions
- ✅ break and continue statements
- ✅ Nested control structures
- ✅ All boolean and comparison operators

These examples compile and run successfully with the current `neurc` implementation, demonstrating real working NEURO control flow patterns.