//! Example: Basic Pattern Matching
//!
//! Purpose: Demonstrates fundamental match expression syntax and patterns
//!
//! Concepts covered:
//! - Basic match expression syntax
//! - Literal patterns (integers, booleans)
//! - Wildcard pattern (_)
//! - Match expressions as return values
//! - Pattern matching in different contexts
//!
//! Expected output: Results from various pattern matching scenarios
//! Compilation: neurc run examples/06_pattern_matching/01_basic_patterns.nr

// Simple integer pattern matching
fn classify_number(x: int) -> int {
    return match x {
        0 => 100,      // Zero case
        1 => 200,      // One case
        2 => 300,      // Two case
        _ => 999       // Default case (everything else)
    };
}

// Boolean pattern matching
fn bool_to_int(flag: bool) -> int {
    return match flag {
        true => 1,     // True case
        false => 0     // False case
    };
}

// Multiple specific values with same result
fn day_type(day: int) -> int {
    return match day {
        1 => 1,        // Monday - weekday
        2 => 1,        // Tuesday - weekday
        3 => 1,        // Wednesday - weekday
        4 => 1,        // Thursday - weekday
        5 => 1,        // Friday - weekday
        6 => 2,        // Saturday - weekend
        7 => 2,        // Sunday - weekend
        _ => 0         // Invalid day
    };
}

// Pattern matching with calculations in arms
fn grade_points(letter_grade: int) -> int {
    return match letter_grade {
        4 => 4 * 100,      // A: 400 points
        3 => 3 * 100,      // B: 300 points
        2 => 2 * 100,      // C: 200 points
        1 => 1 * 100,      // D: 100 points
        0 => 0,            // F: 0 points
        _ => -1            // Invalid grade
    };
}

// Nested match expressions
fn complex_categorization(x: int, y: int) -> int {
    return match x {
        0 => match y {
            0 => 10,       // x=0, y=0
            1 => 20,       // x=0, y=1
            _ => 30        // x=0, y=other
        },
        1 => match y {
            0 => 40,       // x=1, y=0
            1 => 50,       // x=1, y=1
            _ => 60        // x=1, y=other
        },
        _ => 99            // x=other
    };
}

// Match expression in variable assignment
fn assignment_match_demo() -> int {
    let value = 5;
    let category = match value {
        0 => 10,
        5 => 50,
        10 => 100,
        _ => 1
    };
    return category;
}

// Match expression as function arguments
fn add(a: int, b: int) -> int {
    return a + b;
}

fn function_arg_match_demo() -> int {
    let input = 3;
    return add(
        match input {
            3 => 10,
            _ => 0
        },
        match input {
            3 => 20,
            _ => 5
        }
    );
}

// Pattern matching with range-like behavior using multiple patterns
fn temperature_category(temp: int) -> int {
    return match temp {
        0 => 1,        // Freezing temperatures
        5 => 2,        // Cold temperatures
        10 => 2,
        15 => 2,
        20 => 3,       // Mild temperatures
        25 => 3,
        30 => 4,       // Hot temperatures
        35 => 4,
        40 => 4,
        _ => 0         // All other temperatures (including negatives)
    };
}

// Pattern matching for state machines
fn state_transition(current_state: int, input: int) -> int {
    return match current_state {
        1 => match input {      // State 1 transitions
            0 => 1,             // Stay in state 1
            1 => 2,             // Go to state 2
            _ => 0              // Error state
        },
        2 => match input {      // State 2 transitions
            0 => 1,             // Go to state 1
            1 => 3,             // Go to state 3
            _ => 0              // Error state
        },
        3 => match input {      // State 3 transitions
            0 => 1,             // Go to state 1
            1 => 1,             // Go to state 1
            _ => 0              // Error state
        },
        _ => 0                  // Invalid state
    };
}

// Comprehensive wildcard usage
fn wildcard_examples(value: int) -> int {
    return match value {
        1 => 10,       // Specific case
        2 => 20,       // Specific case
        3 => 30,       // Specific case
        _ => 999       // Everything else (wildcard)
    };
}

// Match with mathematical expressions
fn expression_match_demo(base: int) -> int {
    let computed = base * 2;
    return match computed {
        10 => 1,       // base was 5
        20 => 2,       // base was 10
        30 => 3,       // base was 15
        _ => 0         // other values
    };
}

fn main() -> int {
    // Test basic number classification
    let class_0 = classify_number(0);    // 100
    let class_1 = classify_number(1);    // 200
    let class_2 = classify_number(2);    // 300
    let class_other = classify_number(5); // 999

    print(class_0);     // Output: 100
    print(class_1);     // Output: 200
    print(class_2);     // Output: 300
    print(class_other); // Output: 999

    // Test boolean matching
    let bool_true = bool_to_int(true);   // 1
    let bool_false = bool_to_int(false); // 0

    print(bool_true);   // Output: 1
    print(bool_false);  // Output: 0

    // Test day type classification
    let weekday = day_type(3);     // 1 (Wednesday)
    let weekend = day_type(6);     // 2 (Saturday)
    let invalid = day_type(8);     // 0 (invalid)

    print(weekday);  // Output: 1
    print(weekend);  // Output: 2
    print(invalid);  // Output: 0

    // Test grade points
    let grade_a = grade_points(4);     // 400
    let grade_c = grade_points(2);     // 200
    let grade_invalid = grade_points(5); // -1

    print(grade_a);       // Output: 400
    print(grade_c);       // Output: 200
    print(grade_invalid); // Output: -1

    // Test nested matching
    let nested_1 = complex_categorization(0, 1);  // 20
    let nested_2 = complex_categorization(1, 0);  // 40
    let nested_3 = complex_categorization(2, 5);  // 99

    print(nested_1);  // Output: 20
    print(nested_2);  // Output: 40
    print(nested_3);  // Output: 99

    // Test assignment with match
    let assignment_result = assignment_match_demo();  // 50
    print(assignment_result);  // Output: 50

    // Test function arguments with match
    let func_arg_result = function_arg_match_demo();  // 30 (10 + 20)
    print(func_arg_result);  // Output: 30

    // Test temperature categories
    let temp_cold = temperature_category(0);   // 1
    let temp_mild = temperature_category(20);  // 3
    let temp_hot = temperature_category(35);   // 4
    let temp_extreme = temperature_category(50); // 0

    print(temp_cold);    // Output: 1
    print(temp_mild);    // Output: 3
    print(temp_hot);     // Output: 4
    print(temp_extreme); // Output: 0

    // Test state machine
    let state_1_to_2 = state_transition(1, 1);  // 2
    let state_2_to_3 = state_transition(2, 1);  // 3
    let state_3_to_1 = state_transition(3, 0);  // 1

    print(state_1_to_2);  // Output: 2
    print(state_2_to_3);  // Output: 3
    print(state_3_to_1);  // Output: 1

    // Test wildcard patterns
    let wild_specific = wildcard_examples(2);   // 20
    let wild_default = wildcard_examples(100); // 999

    print(wild_specific);  // Output: 20
    print(wild_default);   // Output: 999

    // Test expression matching
    let expr_match = expression_match_demo(10);  // 2 (10*2=20 matches)
    print(expr_match);  // Output: 2

    // Demonstrate match in complex expressions
    let complex_expr = classify_number(1) + wildcard_examples(3);  // 200 + 30 = 230
    print(complex_expr);  // Output: 230

    return 0;
}