// NEURO Calculator Application
// Demonstrates importing and using modules

import "./math";
import "./utils";

/// Advanced calculator function that uses imported modules
fn calculate_advanced(a: int, b: int, operation: string) -> int {
    if operation == "add" {
        // Use math module function
        add(a, b)
    } else if operation == "multiply" {
        // Use math module function  
        multiply(a, b)
    } else if operation == "power" {
        // Use math module function
        power(a, b)
    } else if operation == "max" {
        // Use utils module function
        max(a, b)
    } else if operation == "min" {
        // Use utils module function
        min(a, b)
    } else {
        0
    }
}

/// Demonstrate factorial calculation
fn demo_factorial(n: int) -> int {
    print("Calculating factorial of " + to_string(n));
    factorial(n)
}

/// Main function demonstrating module usage
fn main() -> int {
    let result1 = calculate_advanced(5, 3, "add");
    let result2 = calculate_advanced(4, 2, "power");
    let result3 = demo_factorial(5);
    
    print("5 + 3 = " + to_string(result1));
    print("4 ^ 2 = " + to_string(result2));
    print("5! = " + to_string(result3));
    
    0
}