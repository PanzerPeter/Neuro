//! Example: Basic Function Definitions
//!
//! Purpose: Demonstrates fundamental function syntax and calling conventions
//!
//! Concepts covered:
//! - Function definition syntax
//! - Parameters and return types
//! - Function calls
//! - Void functions (no return value)
//! - Functions with return values
//!
//! Expected output: Various function results
//! Compilation: neurc run examples/02_functions/01_basic_functions.nr

// Function with no parameters, returns an integer
fn get_magic_number() -> int {
    return 42;
}

// Function with parameters, returns a value
fn add(a: int, b: int) -> int {
    return a + b;
}

// Function with parameters, returns a value (different name style)
fn multiply_by_two(value: int) -> int {
    let result = value * 2;
    return result;
}

// Void function (no explicit return type)
fn print_separator() {
    print(999);  // Using print as a separator marker
    return;      // Explicit void return (optional)
}

// Void function without explicit return
fn print_header() {
    print(100);  // Header marker
    print(200);  // Second header marker
    // Implicit void return
}

// Function that uses other functions
fn calculate_sum(x: int, y: int, z: int) -> int {
    let partial_sum = add(x, y);           // Call add function
    let final_sum = add(partial_sum, z);   // Call add again
    return final_sum;
}

// Function with more complex logic
fn absolute_value(n: int) -> int {
    if n < 0 {
        return -n;  // Return negative of negative number
    } else {
        return n;   // Return positive number unchanged
    }
}

// Function demonstrating multiple return paths
fn sign(value: int) -> int {
    if value > 0 {
        return 1;   // Positive
    } else {
        if value < 0 {
            return -1;  // Negative
        } else {
            return 0;   // Zero
        }
    }
}

fn main() -> int {
    // Call function with no parameters
    let magic = get_magic_number();
    print(magic);  // Output: 42

    // Call function with parameters
    let sum = add(10, 15);
    print(sum);  // Output: 25

    // Call function and use result in expression
    let double_value = multiply_by_two(7);
    print(double_value);  // Output: 14

    // Call void functions
    print_separator();  // Prints separator markers
    print_header();     // Prints header markers

    // Chain function calls
    let complex_sum = calculate_sum(5, 10, 3);
    print(complex_sum);  // Output: 18

    // Test absolute value function
    let abs_positive = absolute_value(15);   // 15
    let abs_negative = absolute_value(-8);   // 8
    let abs_zero = absolute_value(0);        // 0

    print(abs_positive);  // Output: 15
    print(abs_negative);  // Output: 8
    print(abs_zero);      // Output: 0

    // Test sign function
    let sign_pos = sign(42);    // 1
    let sign_neg = sign(-17);   // -1
    let sign_zero = sign(0);    // 0

    print(sign_pos);   // Output: 1
    print(sign_neg);   // Output: -1
    print(sign_zero);  // Output: 0

    // Use function results in calculations
    let combined = add(abs_positive, abs_negative);  // 15 + 8 = 23
    print(combined);  // Output: 23

    // Nest function calls
    let nested_result = multiply_by_two(add(3, 4));  // 2 * (3 + 4) = 14
    print(nested_result);  // Output: 14

    return 0;  // Success
}