//! Example: Operators and Expressions
//!
//! Purpose: Demonstrates all available operators and expression evaluation
//!
//! Concepts covered:
//! - Arithmetic operators (+, -, *, /, %)
//! - Comparison operators (<, <=, >, >=, ==, !=)
//! - Logical operators (&&, ||, !)
//! - Unary operators (-, !)
//! - Operator precedence and associativity
//! - Parentheses for grouping
//!
//! Expected output: Results demonstrating each operator type
//! Compilation: neurc run examples/01_basics/05_operators.nr

fn main() -> int {
    // Arithmetic operators
    let a = 10;
    let b = 3;

    let addition = a + b;          // 13
    let subtraction = a - b;       // 7
    let multiplication = a * b;    // 30
    let division = a / b;          // 3 (integer division)
    let modulo = a % b;            // 1 (remainder)

    print(addition);        // Output: 13
    print(subtraction);     // Output: 7
    print(multiplication);  // Output: 30
    print(division);        // Output: 3
    print(modulo);          // Output: 1

    // Unary operators
    let positive = 42;
    let negative = -positive;      // Unary minus: -42
    let double_negative = -negative; // Double negative: 42

    print(negative);        // Output: -42
    print(double_negative); // Output: 42

    // Comparison operators
    let x = 15;
    let y = 20;

    let less_than = x < y;         // true
    let less_equal = x <= y;       // true
    let greater_than = x > y;      // false
    let greater_equal = x >= y;    // false
    let equal = x == y;            // false
    let not_equal = x != y;        // true

    // Note: Boolean results are demonstrated through conditional logic
    // since direct boolean printing may not be fully implemented

    // Logical operators with booleans
    let flag1 = true;
    let flag2 = false;

    let logical_and = flag1 && flag2;    // false
    let logical_or = flag1 || flag2;     // true
    let logical_not = !flag1;            // false

    // Complex logical expressions
    let complex_condition = (x > 10) && (y < 25) || (x == 0);  // true

    // Operator precedence demonstration
    // Precedence (highest to lowest):
    // 1. Unary: -, !
    // 2. Multiplicative: *, /, %
    // 3. Additive: +, -
    // 4. Comparison: <, <=, >, >=
    // 5. Equality: ==, !=
    // 6. Logical AND: &&
    // 7. Logical OR: ||

    let precedence_example1 = 2 + 3 * 4;           // 14 (not 20)
    let precedence_example2 = (2 + 3) * 4;         // 20 (parentheses override)
    let precedence_example3 = 10 > 5 + 3;          // false (5 + 3 = 8, 10 > 8 = true)
    let precedence_example4 = (10 > 5) + 3;        // Invalid (can't add bool + int)

    print(precedence_example1);  // Output: 14
    print(precedence_example2);  // Output: 20

    // Parentheses for grouping
    let grouped1 = (10 + 5) * 2;          // 30
    let grouped2 = 10 + (5 * 2);          // 20
    let grouped3 = (10 - 5) / (3 - 1);    // 2

    print(grouped1);  // Output: 30
    print(grouped2);  // Output: 20
    print(grouped3);  // Output: 2

    // Chained comparisons (not supported - must use logical operators)
    let value = 15;
    let in_range = (value >= 10) && (value <= 20);  // true

    // Assignment with operators
    let mut accumulator = 10;
    accumulator = accumulator + 5;         // 15
    accumulator = accumulator * 2;         // 30
    accumulator = accumulator - 10;        // 20

    print(accumulator);  // Output: 20

    // Complex expression evaluation
    let complex_math = ((10 + 5) * 3 - 20) / 5;     // 5
    // Step by step: (15 * 3 - 20) / 5 = (45 - 20) / 5 = 25 / 5 = 5

    print(complex_math);  // Output: 5

    // Return a computed value
    return precedence_example1 + precedence_example2;  // 14 + 20 = 34
}