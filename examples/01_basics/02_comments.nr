//! Example: Comments and Documentation
//!
//! Purpose: Demonstrates different comment styles and documentation practices
//!
//! Concepts covered:
//! - Line comments with //
//! - Block comments with /* */
//! - Nested block comments
//! - Comment placement strategies
//! - Documentation conventions
//!
//! Expected output: 55
//! Compilation: neurc run examples/01_basics/02_comments.nr

/*
 * Program: Comment Demonstration
 *
 * This program shows how to use comments effectively in NEURO.
 * Comments are completely ignored by the compiler and have zero
 * runtime overhead.
 */

// Function to add two numbers
// Parameters are documented inline
fn add(a: int, b: int) -> int {
    // Line comments can appear anywhere
    let result = a + b;  // End-of-line comments explain specific operations

    /* Block comments can span multiple lines
     * and are useful for longer explanations
     * or temporarily disabling code blocks */

    return result;  // Return the calculated sum
}

/*
 * Block comments can span multiple lines
 * and are useful for longer explanations.
 */

fn main() -> int {
    // Declare variables with descriptive comments
    let x = 42;    // First number
    let y = 13;    // Second number

    /*
     * Calculate the sum using our add function
     * This demonstrates function calls and variable usage
     */
    let sum = add(x, y);

    // Display the result
    print(sum);    // Should output: 55

    // Program completed successfully
    return 0;
}