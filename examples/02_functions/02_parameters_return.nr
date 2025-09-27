//! Example: Parameters and Return Types
//!
//! Purpose: Demonstrates function parameter patterns and return type usage
//!
//! Concepts covered:
//! - Different parameter count patterns
//! - Type annotations for parameters
//! - Various return types
//! - Parameter usage within functions
//! - Return value patterns
//!
//! Expected output: Results demonstrating parameter and return patterns
//! Compilation: neurc run examples/02_functions/02_parameters_return.nr

// Function with single parameter
fn square(n: int) -> int {
    return n * n;
}

// Function with multiple parameters
fn rectangle_area(width: int, height: int) -> int {
    return width * height;
}

// Function with many parameters
fn sum_five(a: int, b: int, c: int, d: int, e: int) -> int {
    return a + b + c + d + e;
}

// Function returning different values based on input
fn categorize_age(age: int) -> int {
    if age < 13 {
        return 1;  // Child
    } else {
        if age < 20 {
            return 2;  // Teenager
        } else {
            if age < 65 {
                return 3;  // Adult
            } else {
                return 4;  // Senior
            }
        }
    }
}

// Function with boolean return type
fn is_even(number: int) -> bool {
    return (number % 2) == 0;
}

// Function demonstrating parameter reuse
fn triangle_area_times_two(base: int, height: int) -> int {
    // Use each parameter multiple times
    let area = (base * height) / 2;
    let doubled = area * 2;
    return doubled;
}

// Function with complex parameter usage
fn calculate_distance_squared(x1: int, y1: int, x2: int, y2: int) -> int {
    let dx = x2 - x1;
    let dy = y2 - y1;
    let distance_squared = (dx * dx) + (dy * dy);
    return distance_squared;
}

// Function demonstrating parameter order importance
fn subtract(minuend: int, subtrahend: int) -> int {
    return minuend - subtrahend;
}

// Function with mixed parameter usage
fn weighted_average(value1: int, weight1: int, value2: int, weight2: int) -> int {
    let weighted_sum = (value1 * weight1) + (value2 * weight2);
    let total_weight = weight1 + weight2;
    return weighted_sum / total_weight;  // Integer division
}

fn main() -> int {
    // Single parameter function calls
    let square_5 = square(5);
    let square_12 = square(12);
    print(square_5);   // Output: 25
    print(square_12);  // Output: 144

    // Multiple parameter function calls
    let rect_area = rectangle_area(8, 6);
    print(rect_area);  // Output: 48

    // Many parameter function calls
    let sum_result = sum_five(1, 2, 3, 4, 5);
    print(sum_result);  // Output: 15

    // Function with conditional returns
    let child_category = categorize_age(8);     // 1
    let teen_category = categorize_age(16);     // 2
    let adult_category = categorize_age(35);    // 3
    let senior_category = categorize_age(70);   // 4

    print(child_category);   // Output: 1
    print(teen_category);    // Output: 2
    print(adult_category);   // Output: 3
    print(senior_category);  // Output: 4

    // Boolean return type usage
    let even_check_6 = is_even(6);    // true
    let even_check_7 = is_even(7);    // false

    // Note: Direct boolean printing may not be fully implemented
    // We can demonstrate boolean usage through conditional logic
    if even_check_6 {
        print(1);  // Represents true
    } else {
        print(0);  // Represents false
    }

    if even_check_7 {
        print(1);  // Represents true
    } else {
        print(0);  // Represents false
    }
    // Expected outputs: 1, 0

    // Complex parameter usage
    let triangle_doubled = triangle_area_times_two(10, 6);
    print(triangle_doubled);  // Output: 60 (area = 30, doubled = 60)

    // Distance calculation
    let dist_sq = calculate_distance_squared(1, 1, 4, 5);  // (3^2 + 4^2 = 9 + 16 = 25)
    print(dist_sq);  // Output: 25

    // Parameter order demonstration
    let sub1 = subtract(10, 3);  // 10 - 3 = 7
    let sub2 = subtract(3, 10);  // 3 - 10 = -7
    print(sub1);  // Output: 7
    print(sub2);  // Output: -7

    // Weighted average calculation
    let avg = weighted_average(80, 3, 90, 2);  // (80*3 + 90*2) / (3+2) = 420/5 = 84
    print(avg);  // Output: 84

    // Using function results as parameters to other functions
    let nested = square(subtract(8, 3));  // square(5) = 25
    print(nested);  // Output: 25

    // Chaining multiple function calls
    let complex = sum_five(
        square(2),           // 4
        rectangle_area(3, 2), // 6
        subtract(10, 5),     // 5
        categorize_age(25),  // 3
        1                    // 1
    );  // Total: 4 + 6 + 5 + 3 + 1 = 19
    print(complex);  // Output: 19

    return 0;
}