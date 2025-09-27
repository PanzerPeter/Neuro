//! Example: Literal Values
//!
//! Purpose: Demonstrates all supported literal types in NEURO
//!
//! Concepts covered:
//! - Integer literals (positive, negative, zero)
//! - Float literals (decimal notation)
//! - String literals with escape sequences
//! - Boolean literals (true/false)
//! - Literal usage in expressions
//!
//! Expected output: Various values demonstrating each literal type
//! Compilation: neurc run examples/01_basics/03_literals.nr

fn main() -> int {
    // Integer literals (32-bit signed integers)
    let positive_int = 42;        // Positive integer
    let zero_int = 0;             // Zero
    let negative_int = -15;       // Negative integer (using unary minus)

    print(positive_int);  // Output: 42
    print(zero_int);      // Output: 0
    print(negative_int);  // Output: -15

    // Float literals (64-bit IEEE 754 double-precision)
    let positive_float = 3.14;    // Positive float
    let zero_float = 0.0;         // Zero float (must have decimal point)
    let negative_float = -2.5;    // Negative float

    // Note: NEURO currently focuses on integer operations in examples
    // Float operations are parsed but may have limited runtime support

    // String literals (UTF-8 encoded)
    let simple_string = "hello";           // Basic string
    let empty_string = "";                 // Empty string
    let escaped_string = "line1\nline2";   // String with newline escape
    let quoted_string = "She said \"Hi\""; // String with embedded quotes
    let path_string = "C:\\Users\\file";   // String with backslash escapes

    // Note: String operations are parsed but runtime string handling
    // is still being developed in the current implementation

    // Boolean literals
    let true_value = true;     // Boolean true
    let false_value = false;   // Boolean false

    // Using literals in expressions
    let math_result = 10 + 5 * 2;        // Integer arithmetic: 20
    let comparison = 15 > 10;            // Boolean expression: true
    let logical_op = true && false;      // Logical operation: false

    print(math_result);  // Output: 20

    // Literals can be used directly in function calls
    print(100);          // Direct integer literal

    // Return an integer literal
    return 1;            // Success exit code
}