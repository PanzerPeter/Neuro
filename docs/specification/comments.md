# Comments

NEURO supports two types of comments for code documentation and explanation: line comments and block comments. Comments are ignored by the compiler and do not affect program execution.

## Line Comments

### Syntax
Line comments start with `//` and continue until the end of the line:

```neuro
// This is a line comment
```

### Usage
Line comments are ideal for:
- Single-line explanations
- Temporarily disabling code
- End-of-line annotations

### Examples
```neuro
// Function to calculate factorial
fn factorial(n: int) -> int {
    let mut result = 1;        // Initialize accumulator
    let mut i = 1;             // Loop counter

    while i <= n {
        result = result * i;   // Multiply by current number
        i = i + 1;            // Increment counter
    }

    return result;            // Return calculated result
}
```

## Block Comments

### Syntax
Block comments start with `/*` and end with `*/`:

```neuro
/* This is a block comment */
```

Block comments can span multiple lines:

```neuro
/*
 * Multi-line block comment
 * with multiple lines of text
 */
```

### Usage
Block comments are useful for:
- Multi-line explanations
- Function/module documentation
- Temporarily commenting out large blocks of code

### Examples
```neuro
/*
 * Calculates the greatest common divisor (GCD) of two integers
 * using the Euclidean algorithm.
 *
 * Parameters:
 * - a: First integer
 * - b: Second integer
 *
 * Returns: The GCD of a and b
 */
fn gcd(a: int, b: int) -> int {
    let mut x = a;
    let mut y = b;

    while y != 0 {
        let temp = y;
        y = x % y;
        x = temp;
    }

    return x;
}
```

## Comment Placement

Comments can appear in various locations:

### Before Declarations
```neuro
// Main program entry point
fn main() -> int {
    return 0;
}
```

### Within Functions
```neuro
fn process_data() -> int {
    // Initialize variables
    let mut count = 0;

    /* Process data here
       Multiple steps involved */
    count = count + 1;

    return count;  // Return final count
}
```

### Between Statements
```neuro
fn example() -> int {
    let x = 10;

    // Check if x is positive
    if x > 0 {
        return x;
    }

    // Default case
    return 0;
}
```

## Nested Comments

NEURO supports proper nesting of block comments:

```neuro
/*
 * Outer comment
 * /* Inner comment */
 * More outer comment
 */
fn example() {
    return;
}
```

## Comments and Tokenization

Comments are stripped during lexical analysis:
- Line comments are removed from `//` to the end of line
- Block comments are removed from `/*` to matching `*/`
- The tokenizer continues processing after comments
- Comments do not generate tokens in the output

## Documentation Style Recommendations

### Function Documentation
```neuro
/*
 * Brief description of what the function does.
 *
 * Parameters:
 * - param1: Description of first parameter
 * - param2: Description of second parameter
 *
 * Returns: Description of return value
 *
 * Example:
 * let result = my_function(10, 20);
 */
fn my_function(param1: int, param2: int) -> int {
    // Implementation details
    return param1 + param2;
}
```

### Code Sections
```neuro
fn complex_algorithm() -> int {
    // Phase 1: Initialize data structures
    let mut data = 0;

    /* Phase 2: Process data
     * This section handles the main computation
     * using a multi-step algorithm */
    data = data + 1;

    // Phase 3: Return results
    return data;
}
```

## Complete Example

```neuro
// Example program demonstrating NEURO comment syntax
/*
 * Program: Comment Demo
 * Author: NEURO Team
 * Purpose: Show different comment styles
 */

fn main() -> int {
    // Declare and initialize variables
    let x = 42;    // Magic number
    let y = 13;    // Another number

    /*
     * Calculate sum using simple addition
     * Both variables are integers
     */
    let sum = x + y;

    // Return the calculated sum
    return sum;    /* Final result: 55 */
}
```

## Current Implementation Status

### Fully Implemented ✅
- Line comments with `//` syntax
- Block comments with `/* */` syntax
- Proper comment termination parsing
- Multi-line block comments
- Nested block comments
- Comments stripped from token stream

### Current Limitations
- Documentation comments (like `///` or `/** */`) not yet implemented
- Comment preservation for tooling/IDE support not available
- Special comment directives not supported

## Performance Notes

- Comments have zero runtime overhead (completely removed during compilation)
- Comment parsing is efficient and does not impact compilation speed
- Large comment blocks do not affect generated code size

