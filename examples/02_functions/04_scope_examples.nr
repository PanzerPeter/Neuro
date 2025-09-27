//! Example: Function Scope and Variable Interactions
//!
//! Purpose: Demonstrates scope rules, parameter passing, and variable visibility
//!
//! Concepts covered:
//! - Function parameter scope
//! - Local variable scope within functions
//! - Variable shadowing in functions
//! - Parameter vs local variable interactions
//! - Block scope within functions
//! - No global variables (function-only scope)
//!
//! Expected output: Results demonstrating scope behavior
//! Compilation: neurc run examples/02_functions/04_scope_examples.nr

// Function demonstrating parameter scope
fn parameter_scope_demo(x: int, y: int) -> int {
    // Parameters x and y are only visible within this function
    let local_var = x + y;  // Local variable using parameters

    // Parameters can be used multiple times
    let another_calc = x * y;

    return local_var + another_calc;
}

// Function showing local variable scope
fn local_variable_scope() -> int {
    let a = 10;  // Local to this function
    let b = 20;  // Local to this function

    // These variables are not accessible from other functions
    let result = a + b;
    return result;
}

// Function demonstrating variable shadowing
fn variable_shadowing(value: int) -> int {
    let value = value * 2;  // Shadows the parameter 'value'
    let value = value + 10; // Shadows the previous local 'value'

    // Now 'value' refers to the most recent declaration
    return value;
}

// Function with block scope inside
fn block_scope_demo(input: int) -> int {
    let outer_var = input;

    {
        // Inner block with its own scope
        let inner_var = outer_var * 2;  // Can access outer_var
        let outer_var = 999;            // Shadows outer outer_var in this block

        // In this block, outer_var = 999, inner_var = input * 2
        print(outer_var);   // Prints the shadowed value: 999
        print(inner_var);   // Prints input * 2
    }

    // Back in outer scope, outer_var returns to original value
    // inner_var is no longer accessible
    return outer_var;  // Returns original input value
}

// Function demonstrating no parameter modification
fn cannot_modify_params(x: int, y: int) -> int {
    // Parameters are immutable - cannot do: x = x + 1;
    // This would cause a compilation error: x = 10;

    // Must use local variables for modifications
    let modified_x = x + 1;
    let modified_y = y * 2;

    return modified_x + modified_y;
}

// Function showing multiple scopes
fn nested_scopes(a: int) -> int {
    let b = a * 2;  // Function scope

    {
        let c = b + 5;  // First inner scope
        {
            let d = c * 3;  // Second inner scope
            print(d);       // Can access all: a, b, c, d
        }
        // d no longer accessible here
        print(c);  // Can access: a, b, c
    }
    // c and d no longer accessible here
    print(b);  // Can access: a, b

    return b;
}

// Helper function for scope isolation demonstration
fn helper_function() -> int {
    let helper_var = 42;  // This is completely separate from other functions
    return helper_var;
}

// Function to show parameter passing doesn't affect originals
fn double_value(n: int) -> int {
    let n = n * 2;  // Shadows parameter with local variable
    return n;
}

// Function demonstrating complex scope interactions
fn complex_scope_demo(base: int) -> int {
    let multiplier = 3;  // Function-level variable

    if base > 0 {
        let adjustment = 10;        // Block-level variable
        let result = base * multiplier + adjustment;

        {
            let final_adjustment = 5;  // Nested block variable
            let result = result + final_adjustment;  // Shadows outer result
            print(result);  // Prints the inner result
        }

        // Back to if-block scope, original result is available
        return result;  // Returns outer result (without final_adjustment)
    } else {
        let adjustment = -5;        // Different block, different adjustment
        let result = base * multiplier + adjustment;
        return result;
    }
}

fn main() -> int {
    // Test parameter scope
    let param_result = parameter_scope_demo(5, 3);  // (5+3) + (5*3) = 8 + 15 = 23
    print(param_result);  // Output: 23

    // Test local variable scope
    let local_result = local_variable_scope();      // 10 + 20 = 30
    print(local_result);  // Output: 30

    // Test variable shadowing
    let shadow_result = variable_shadowing(5);      // ((5*2) + 10) = 20
    print(shadow_result);  // Output: 20

    // Test block scope
    let block_result = block_scope_demo(10);        // Returns original input: 10
    print(block_result);  // Output: 10
    // Note: The function also prints 999 and 20 from the inner block

    // Test parameter immutability
    let param_mod_result = cannot_modify_params(5, 3);  // (5+1) + (3*2) = 12
    print(param_mod_result);  // Output: 12

    // Test nested scopes
    let nested_result = nested_scopes(4);           // 4 * 2 = 8
    print(nested_result);  // Output: 8
    // Note: Function also prints intermediate values

    // Test scope isolation
    let helper_result = helper_function();          // 42
    print(helper_result);  // Output: 42

    // Test that original values aren't affected by function calls
    let original_value = 7;
    let doubled = double_value(original_value);     // 7 * 2 = 14
    print(original_value);  // Still 7 (unchanged)
    print(doubled);         // 14

    // Test complex scope interactions
    let complex_pos = complex_scope_demo(5);        // Positive case
    let complex_neg = complex_scope_demo(-2);       // Negative case
    print(complex_pos);  // Output depends on complex calculation
    print(complex_neg);  // Output depends on complex calculation

    // Demonstrate that variables from one function don't affect another
    let test1 = parameter_scope_demo(1, 2);         // Fresh function call
    let test2 = parameter_scope_demo(3, 4);         // Independent function call
    print(test1);  // Output: (1+2) + (1*2) = 5
    print(test2);  // Output: (3+4) + (3*4) = 19

    // Show that each function maintains its own scope
    let multiple_calls = local_variable_scope() + helper_function();  // 30 + 42 = 72
    print(multiple_calls);  // Output: 72

    return 0;
}