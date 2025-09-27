//! Example: Primitive Types in NEURO
//!
//! Purpose: Demonstrates all primitive types and their usage
//!
//! Concepts covered:
//! - int type (32-bit signed integers)
//! - float type (64-bit IEEE 754)
//! - bool type (true/false)
//! - string type (UTF-8 encoded)
//! - Type annotations vs inference
//! - Type compatibility in operations
//!
//! Expected output: Values demonstrating each type
//! Compilation: neurc run examples/04_types/01_primitive_types.nr

// Function demonstrating integer types
fn integer_examples() -> int {
    // Explicit type annotation
    let explicit_int: int = 42;

    // Type inference from literal
    let inferred_int = 100;

    // Integer arithmetic
    let sum = explicit_int + inferred_int;  // 142
    let product = explicit_int * 2;         // 84
    let difference = explicit_int - 10;     // 32
    let quotient = explicit_int / 6;        // 7 (integer division)
    let remainder = explicit_int % 5;       // 2

    print(sum);       // Output: 142
    print(product);   // Output: 84
    print(difference); // Output: 32
    print(quotient);  // Output: 7
    print(remainder); // Output: 2

    return sum;
}

// Function demonstrating float types
fn float_examples() -> int {
    // Explicit type annotation
    let explicit_float: float = 3.14;

    // Type inference from literal
    let inferred_float = 2.71;

    // Float arithmetic (converted to int for return)
    let sum = explicit_float + inferred_float;     // 5.85
    let product = explicit_float * 2.0;           // 6.28
    let quotient = explicit_float / 2.0;          // 1.57

    // Note: For demonstration, we return 0 since we can't return floats
    // In real usage, you would use float operations appropriately
    print(0);  // Placeholder - float printing may not be fully implemented

    return 0;
}

// Function demonstrating boolean types
fn boolean_examples() -> int {
    // Explicit type annotation
    let explicit_bool: bool = true;

    // Type inference from literal
    let inferred_bool = false;

    // Boolean operations
    let and_result = explicit_bool && inferred_bool;  // false
    let or_result = explicit_bool || inferred_bool;   // true
    let not_result = !explicit_bool;                  // false

    // Using booleans in conditionals
    if explicit_bool {
        print(1);  // This will execute
    }

    if inferred_bool {
        print(2);  // This will not execute
    } else {
        print(3);  // This will execute
    }

    // Converting boolean logic to integers for demonstration
    let bool_to_int = 0;
    if and_result {
        bool_to_int = 10;
    }
    if or_result {
        bool_to_int = bool_to_int + 20;  // 20
    }
    if not_result {
        bool_to_int = bool_to_int + 30;
    }

    print(bool_to_int);  // Output: 20

    return bool_to_int;
}

// Function demonstrating string types
fn string_examples() -> int {
    // Explicit type annotation
    let explicit_string: string = "Hello";

    // Type inference from literal
    let inferred_string = "World";

    // String with escape sequences
    let escaped_string = "Line 1\nLine 2\tTabbed";
    let quoted_string = "She said \"Hello!\"";

    // Note: String operations like concatenation are not yet fully implemented
    // This demonstrates string declaration and basic usage

    // Return length approximation for demonstration
    return 5;  // Approximate length of "Hello"
}

// Function demonstrating type compatibility
fn type_compatibility() -> int {
    // Same types work together
    let int_a = 10;
    let int_b = 20;
    let int_sum = int_a + int_b;  // Valid: int + int = int

    let bool_a = true;
    let bool_b = false;
    let bool_result = bool_a && bool_b;  // Valid: bool && bool = bool

    // Mixed type operations (where valid)
    let comparison_result = int_a > int_b;  // Valid: int comparison returns bool

    print(int_sum);  // Output: 30

    if comparison_result {
        return 1;
    } else {
        return 2;  // This will execute since 10 > 20 is false
    }
}

// Function demonstrating type inference
fn type_inference_examples() -> int {
    // Compiler infers types from literals
    let auto_int = 42;           // Inferred as int
    let auto_float = 3.14;       // Inferred as float
    let auto_bool = true;        // Inferred as bool
    let auto_string = "text";    // Inferred as string

    // Compiler infers types from operations
    let computed_int = auto_int * 2;        // Inferred as int
    let computed_bool = auto_bool && false; // Inferred as bool

    // Type inference with function calls
    let function_result = integer_examples();  // Inferred as int (return type)

    print(computed_int);  // Output: 84
    print(function_result); // Output: 142

    return computed_int + function_result;  // 84 + 142 = 226
}

// Function demonstrating explicit vs inferred types
fn explicit_vs_inferred() -> int {
    // These are equivalent:
    let explicit: int = 100;
    let inferred = 100;

    // Both can be used identically
    let sum1 = explicit + 50;   // 150
    let sum2 = inferred + 50;   // 150

    print(sum1);  // Output: 150
    print(sum2);  // Output: 150

    // Explicit types are useful for clarity
    let user_age: int = 25;
    let is_adult: bool = user_age >= 18;

    if is_adult {
        return 1;
    } else {
        return 0;
    }
}

fn main() -> int {
    // Test all primitive type examples
    let int_result = integer_examples();       // Tests integers
    let float_result = float_examples();       // Tests floats
    let bool_result = boolean_examples();      // Tests booleans
    let string_result = string_examples();     // Tests strings
    let compat_result = type_compatibility();  // Tests type compatibility
    let infer_result = type_inference_examples(); // Tests inference
    let explicit_result = explicit_vs_inferred(); // Tests explicit vs inferred

    // Combine results for final output
    let total = int_result + float_result + bool_result + string_result +
                compat_result + infer_result + explicit_result;

    print(total);  // Output: sum of all results

    return 0;
}