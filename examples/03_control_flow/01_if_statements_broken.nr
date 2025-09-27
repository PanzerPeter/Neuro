//! Example: Conditional Statements (if/else)
//!
//! Purpose: Demonstrates conditional execution with if/else statements
//!
//! Concepts covered:
//! - Basic if statements
//! - if/else statements
//! - else if chains
//! - Nested if statements
//! - Complex boolean conditions
//! - Braces requirement for all blocks
//!
//! Expected output: Results from various conditional tests
//! Compilation: neurc run examples/03_control_flow/01_if_statements.nr

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

// Function demonstrating else if chains
fn else_if_chain(score: int) -> int {
    if score >= 90 {
        return 4;    // A grade (90-100)
    } else if score >= 80 {
        return 3;    // B grade (80-89)
    } else if score >= 70 {
        return 2;    // C grade (70-79)
    } else if score >= 60 {
        return 1;    // D grade (60-69)
    } else {
        return 0;    // F grade (below 60)
    }
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

    // Condition with parentheses for grouping
    if (x > y) && ((z > 0) || (x == 0)) {
        return 4;  // x > y AND (z > 0 OR x == 0)
    }

    return 0;  // Default case
}

// Function demonstrating range checking
fn range_check(value: int) -> int {
    if (value >= 1) && (value <= 10) {
        return 1;  // In range 1-10
    } else if (value >= 11) && (value <= 20) {
        return 2;  // In range 11-20
    } else if (value >= 21) && (value <= 30) {
        return 3;  // In range 21-30
    } else {
        return 0;  // Outside all ranges
    }
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

    // Complex negation
    if !(number > 10) && !(number < 0) {
        return 3;  // number is in range [0, 10]
    }

    return 0;  // Default
}

// Function demonstrating early returns
fn early_return_demo(x: int) -> int {
    // Guard clauses - early returns for special cases
    if x < 0 {
        return -1;  // Early return for negative
    }

    if x == 0 {
        return 0;   // Early return for zero
    }

    if x > 100 {
        return 999; // Early return for large values
    }

    // Main logic continues here
    if x > 50 {
        return 2;
    } else {
        return 1;
    }
}

// Function with multiple nested conditions
fn deep_nesting(a: int, b: int, c: int) -> int {
    if a > 0 {
        if b > 0 {
            if c > 0 {
                return 1;  // All positive
            } else {
                return 2;  // a,b positive, c not
            }
        } else {
            if c > 0 {
                return 3;  // a,c positive, b not
            } else {
                return 4;  // only a positive
            }
        }
    } else {
        if b > 0 {
            if c > 0 {
                return 5;  // b,c positive, a not
            } else {
                return 6;  // only b positive
            }
        } else {
            if c > 0 {
                return 7;  // only c positive
            } else {
                return 8;  // none positive
            }
        }
    }
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

    // Test else if chains
    let grade_a = else_if_chain(95);      // 4 (A grade)
    let grade_b = else_if_chain(85);      // 3 (B grade)
    let grade_c = else_if_chain(75);      // 2 (C grade)
    let grade_d = else_if_chain(65);      // 1 (D grade)
    let grade_f = else_if_chain(45);      // 0 (F grade)

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

    // Test range checking
    let range_1 = range_check(5);     // 1 (in range 1-10)
    let range_2 = range_check(15);    // 2 (in range 11-20)
    let range_3 = range_check(25);    // 3 (in range 21-30)
    let range_0 = range_check(35);    // 0 (outside all ranges)

    print(range_1);  // Output: 1
    print(range_2);  // Output: 2
    print(range_3);  // Output: 3
    print(range_0);  // Output: 0

    // Test negation
    let neg_1 = negation_demo(false, 10);  // 1 (!flag is true)
    let neg_2 = negation_demo(true, 3);    // 2 (!(3 > 5) is true)

    print(neg_1);  // Output: 1
    print(neg_2);  // Output: 2

    // Test early returns
    let early_1 = early_return_demo(-5);   // -1 (negative)
    let early_2 = early_return_demo(0);    // 0 (zero)
    let early_3 = early_return_demo(150);  // 999 (> 100)
    let early_4 = early_return_demo(75);   // 2 (> 50)
    let early_5 = early_return_demo(25);   // 1 (<= 50)

    print(early_1);  // Output: -1
    print(early_2);  // Output: 0
    print(early_3);  // Output: 999
    print(early_4);  // Output: 2
    print(early_5);  // Output: 1

    // Test deep nesting
    let deep = deep_nesting(1, 1, 1);      // 1 (all positive)
    print(deep);  // Output: 1

    return 0;
}