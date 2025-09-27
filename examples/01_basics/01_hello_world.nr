//! Example: Hello World - Your First NEURO Program
//!
//! Purpose: Demonstrates the most basic NEURO program structure and output
//!
//! Concepts covered:
//! - Function definition syntax
//! - Main function as program entry point
//! - Return statements and values
//! - Print function for output
//! - Integer literals
//!
//! Expected output: 42
//! Compilation: neurc run examples/01_basics/01_hello_world.nr

// The main function is the entry point of every NEURO program
// It must return an integer value (exit code)
fn main() -> int {
    // Print a number to demonstrate output
    // The print function displays values to the console
    print(42);

    // Return 0 to indicate successful program execution
    // Non-zero return values typically indicate errors
    return 0;
}