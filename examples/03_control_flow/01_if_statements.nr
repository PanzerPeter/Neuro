//! Example: Simple Conditional Statements (if/else)
//!
//! Purpose: Demonstrates conditional execution with if/else statements
//!
//! Concepts covered:
//! - Basic if statements
//! - if/else statements
//! - Nested if statements
//! - Complex boolean conditions
//! - Early return patterns
//!
//! Expected output: Results from various conditional tests
//! Compilation: neurc run examples/03_control_flow/01_if_statements_simple.nr

// Function demonstrating basic if statement
fn basic_if_demo(x: int) -> int {
    if x > 0 {
        return 1;  // Positive case
    }
    return 0;  // Default case (not positive)
}

// Function demonstrating if/else
fn if_else_demo(value: int) -> int {
    if value > 10 {
        return 100;  // High value
    } else {
        return 50;   // Low value
    }
}

// Function using early returns instead of else-if chains
fn grade_classifier(score: int) -> int {
    if score >= 90 {
        return 4;    // A grade (90-100)
    }
    if score >= 80 {
        return 3;    // B grade (80-89)
    }
    if score >= 70 {
        return 2;    // C grade (70-79)
    }
    if score >= 60 {
        return 1;    // D grade (60-69)
    }
    return 0;        // F grade (below 60)
}

// Function demonstrating nested if statements
fn nested_if_demo(a: int, b: int) -> int {
    if a > 0 {
        if b > 0 {
            return 1;    // Both positive
        } else {
            return 2;    // a positive, b not positive
        }
    } else {
        if b > 0 {
            return 3;    // a not positive, b positive
        } else {
            return 4;    // Neither positive
        }
    }
}

// Function demonstrating complex boolean conditions
fn complex_conditions(x: int, y: int, z: int) -> int {
    // Using logical AND
    if (x > 0) && (y > 0) {
        return 1;  // Both x and y are positive
    }

    // Using logical OR
    if (x < 0) || (y < 0) {
        return 2;  // At least one of x or y is negative
    }

    // Complex condition with multiple operators
    if ((x + y) > 10) && (z <= 5) {
        return 3;  // Sum of x and y is greater than 10, and z is small
    }

    return 0;  // Default case
}

// Function demonstrating negation in conditions
fn negation_demo(flag: bool, number: int) -> int {
    // Using logical NOT with boolean
    if !flag {
        return 1;  // flag is false
    }

    // Using NOT with comparison
    if !(number > 5) {
        return 2;  // number is NOT greater than 5 (i.e., <= 5)
    }

    return 0;  // Default
}

// Function with multiple return paths using early returns
fn value_classifier(x: int) -> int {
    // Guard clauses - early returns for special cases
    if x < 0 {
        return 1;  // Negative numbers
    }

    if x == 0 {
        return 2;  // Zero
    }

    if x <= 10 {
        return 3;  // Small positive (1-10)
    }

    if x <= 100 {
        return 4;  // Medium positive (11-100)
    }

    return 5;      // Large positive (>100)
}

fn main() -> int {
    // Test basic if statements
    let basic_pos = basic_if_demo(5);     // 1 (positive)
    let basic_neg = basic_if_demo(-3);    // 0 (not positive)
    let basic_zero = basic_if_demo(0);    // 0 (not positive)

    print(basic_pos);   // Output: 1
    print(basic_neg);   // Output: 0
    print(basic_zero);  // Output: 0

    // Test if/else statements
    let high_value = if_else_demo(15);    // 100 (> 10)
    let low_value = if_else_demo(5);      // 50 (<= 10)

    print(high_value);  // Output: 100
    print(low_value);   // Output: 50

    // Test grade classification with early returns
    let grade_a = grade_classifier(95);   // 4 (A grade)
    let grade_b = grade_classifier(85);   // 3 (B grade)
    let grade_c = grade_classifier(75);   // 2 (C grade)
    let grade_d = grade_classifier(65);   // 1 (D grade)
    let grade_f = grade_classifier(45);   // 0 (F grade)

    print(grade_a);  // Output: 4
    print(grade_b);  // Output: 3
    print(grade_c);  // Output: 2
    print(grade_d);  // Output: 1
    print(grade_f);  // Output: 0

    // Test nested if statements
    let nested_1 = nested_if_demo(5, 3);    // 1 (both positive)
    let nested_2 = nested_if_demo(5, -1);   // 2 (a pos, b not)
    let nested_3 = nested_if_demo(-2, 3);   // 3 (a not, b pos)
    let nested_4 = nested_if_demo(-1, -2);  // 4 (neither pos)

    print(nested_1);  // Output: 1
    print(nested_2);  // Output: 2
    print(nested_3);  // Output: 3
    print(nested_4);  // Output: 4

    // Test complex conditions
    let complex_1 = complex_conditions(5, 3, 2);   // 1 (both x,y positive)
    let complex_2 = complex_conditions(-1, 5, 7);  // 2 (x negative)
    let complex_3 = complex_conditions(6, 5, 4);   // 3 (x+y > 10, z <= 5)

    print(complex_1);  // Output: 1
    print(complex_2);  // Output: 2
    print(complex_3);  // Output: 3

    // Test negation
    let neg_1 = negation_demo(false, 10);  // 1 (!false is true)
    let neg_2 = negation_demo(true, 3);    // 2 (!(3 > 5) is true)

    print(neg_1);  // Output: 1
    print(neg_2);  // Output: 2

    // Test value classification
    let class_neg = value_classifier(-5);    // 1 (negative)
    let class_zero = value_classifier(0);    // 2 (zero)
    let class_small = value_classifier(8);   // 3 (small positive)
    let class_med = value_classifier(50);    // 4 (medium positive)
    let class_large = value_classifier(200); // 5 (large positive)

    print(class_neg);    // Output: 1
    print(class_zero);   // Output: 2
    print(class_small);  // Output: 3
    print(class_med);    // Output: 4
    print(class_large);  // Output: 5

    return 0;
}