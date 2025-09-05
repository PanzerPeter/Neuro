// Basic NEURO expressions example
// This file demonstrates the expression evaluation capabilities

fn calculate_area(width, height) -> float {
    return width * height;
}

fn demonstrate_arithmetic() {
    // Basic arithmetic
    let sum = 10 + 5;
    let difference = 20 - 8;
    let product = 6 * 7;
    let quotient = 15 / 3;
    let remainder = 17 % 5;
    
    // Mixed types
    let mixed = 3 + 2.5;
    
    // String concatenation
    let greeting = "Hello" + " World";
    
    // Comparisons
    let is_greater = 10 > 5;
    let is_equal = 42 == 42;
    
    // Complex expressions with precedence
    let complex = (2 + 3) * 4 - 8 / 2;
    
    // Unary operations
    let negative = -42;
}

fn control_flow_demo() {
    let x = 10;
    
    if x > 5 {
        let message = "x is greater than 5";
    } else {
        let message = "x is not greater than 5";
    }
    
    while x > 0 {
        x = x - 1;
        if x == 5 {
            break;
        }
    }
}

fn main() {
    demonstrate_arithmetic();
    control_flow_demo();
}