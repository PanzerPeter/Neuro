//! Example: Variables and Mutability
//!
//! Purpose: Demonstrates variable declarations, type annotations, and mutability
//!
//! Concepts covered:
//! - Immutable variables with let
//! - Mutable variables with let mut
//! - Type inference vs explicit type annotations
//! - Variable shadowing
//! - Variable scope in blocks
//! - Assignment and reassignment
//!
//! Expected output: 25
//! Compilation: neurc run examples/01_basics/04_variables.nr

fn main() -> int {
    // Immutable variables (default behavior)
    let x = 42;                    // Type inferred as int
    let y: int = 10;              // Explicit type annotation
    let message = "hello";         // Type inferred as string
    let flag: bool = true;         // Explicit boolean type

    print(x);  // Output: 42
    print(y);  // Output: 10

    // Mutable variables (can be reassigned)
    let mut counter = 0;           // Mutable integer
    let mut total: int = 100;      // Mutable with explicit type

    // Reassigning mutable variables
    counter = 5;                   // First assignment
    counter = counter + 1;         // Using current value: 6
    total = total - 50;            // total becomes 50

    print(counter);  // Output: 6
    print(total);    // Output: 50

    // Variable shadowing (redeclaring variables in same scope)
    let value = 10;                // First declaration
    let value = value * 2;         // Shadows previous value: 20
    let value = value + 5;         // Shadows again: 25

    print(value);  // Output: 25

    // Block scoping
    {
        let inner_var = 100;       // Only visible in this block
        let value = 999;           // Shadows outer 'value' in this block only

        print(inner_var);  // Output: 100
        print(value);      // Output: 999 (shadowed value)
    }
    // inner_var is no longer accessible here
    // value returns to its outer scope value (25)

    print(value);  // Output: 25 (outer scope value restored)

    // Demonstrating immutability
    let immutable = 42;
    // immutable = 43;  // This would cause a compilation error!

    // Multiple variable declarations
    let a = 1;
    let b = 2;
    let c = 3;
    let sum = a + b + c;           // Using multiple variables

    print(sum);  // Output: 6

    // Mixed mutability patterns
    let mut accumulator = 0;
    let increment = 5;             // Immutable increment value

    accumulator = accumulator + increment;  // 5
    accumulator = accumulator + increment;  // 10
    accumulator = accumulator + increment;  // 15

    print(accumulator);  // Output: 15

    return value;  // Return the final shadowed value: 25
}