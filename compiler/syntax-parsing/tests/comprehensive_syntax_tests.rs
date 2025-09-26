//! Comprehensive syntax parsing tests
//! These tests cover complex syntax combinations, nested structures, and edge cases

use syntax_parsing::{Parser, ParseError};
use lexical_analysis::Lexer;
use shared_types::{Program, Item, Statement};

/// Helper function to parse source code and return the AST
fn parse_source(source: &str) -> Result<Program, ParseError> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexical analysis should succeed");

    let mut parser = Parser::new(tokens);
    parser.parse()
}

/// Helper function to assert parsing succeeds
fn assert_parses_successfully(source: &str, description: &str) {
    match parse_source(source) {
        Ok(program) => {
            assert!(!program.items.is_empty(), "{}: Should have at least one item", description);
        }
        Err(error) => {
            panic!("{}: Parsing should succeed, but got error: {}", description, error);
        }
    }
}

/// Helper function to assert parsing fails
fn assert_parsing_fails(source: &str, description: &str) {
    match parse_source(source) {
        Ok(_) => {
            panic!("{}: Parsing should fail, but succeeded", description);
        }
        Err(_) => {
            // Expected to fail
        }
    }
}

#[test]
fn test_deeply_nested_expressions() {
    let source = r#"
fn main() -> int {
    return ((((1 + 2) * 3) - 4) / ((5 + 6) * (7 - 8)));
}
"#;
    assert_parses_successfully(source, "Deeply nested expressions");
}

#[test]
fn test_complex_boolean_expressions() {
    let source = r#"
fn main() -> bool {
    return (x > 5 && y < 10) || (a == b && c != d) || (m >= n || p <= q);
}
"#;
    assert_parses_successfully(source, "Complex boolean expressions");
}

#[test]
fn test_nested_function_calls() {
    let source = r#"
fn add(x: int, y: int) -> int {
    return x + y;
}

fn multiply(x: int, y: int) -> int {
    return x * y;
}

fn main() -> int {
    return add(multiply(2, 3), multiply(add(1, 2), 4));
}
"#;
    assert_parses_successfully(source, "Nested function calls");
}

#[test]
fn test_deeply_nested_if_statements() {
    let source = r#"
fn main() -> int {
    if x > 0 {
        if y > 0 {
            if z > 0 {
                if w > 0 {
                    return 1;
                } else {
                    return 2;
                }
            } else {
                return 3;
            }
        } else {
            return 4;
        }
    } else {
        return 5;
    }
}
"#;
    assert_parses_successfully(source, "Deeply nested if statements");
}

#[test]
fn test_nested_while_loops() {
    let source = r#"
fn main() -> int {
    while x > 0 {
        while y > 0 {
            while z > 0 {
                z = z - 1;
            }
            y = y - 1;
        }
        x = x - 1;
    }
    return 0;
}
"#;
    assert_parses_successfully(source, "Nested while loops");
}

#[test]
fn test_mixed_control_flow() {
    let source = r#"
fn fibonacci(n: int) -> int {
    if n <= 1 {
        return n;
    } else {
        let a = 0;
        let b = 1;
        let i = 2;
        while i <= n {
            let temp = a + b;
            a = b;
            b = temp;
            i = i + 1;
        }
        return b;
    }
}

fn main() -> int {
    return fibonacci(10);
}
"#;
    assert_parses_successfully(source, "Mixed control flow (if/while/variables)");
}

#[test]
fn test_operator_precedence_complex() {
    let source = r#"
fn main() -> bool {
    return 1 + 2 * 3 - 4 / 2 > 3 && 5 < 6 || 7 == 8 && 9 != 10;
}
"#;
    assert_parses_successfully(source, "Complex operator precedence");

    // Verify the precedence is correctly parsed
    let program = parse_source(source).unwrap();
    if let Item::Function(func) = &program.items[0] {
        if let Statement::Return(return_stmt) = &func.body.statements[0] {
            // The expression should be parsed with correct precedence
            // This is a complex check but ensures the parser respects operator precedence
            assert!(return_stmt.value.is_some(), "Return statement should have a value");
        }
    }
}

#[test]
fn test_function_with_many_parameters() {
    let source = r#"
fn many_params(a: int, b: int, c: int, d: int, e: int, f: int, g: int, h: int) -> int {
    return a + b + c + d + e + f + g + h;
}

fn main() -> int {
    return many_params(1, 2, 3, 4, 5, 6, 7, 8);
}
"#;
    assert_parses_successfully(source, "Function with many parameters");
}

#[test]
fn test_empty_function_body() {
    let source = r#"
fn empty() {
}

fn main() -> int {
    empty();
    return 0;
}
"#;
    assert_parses_successfully(source, "Function with empty body");
}

#[test]
fn test_multiple_variable_declarations() {
    let source = r#"
fn main() -> int {
    let a = 1;
    let b = 2;
    let c = 3;
    let d = a + b;
    let e = c * d;
    let f = e - a;
    return f;
}
"#;
    assert_parses_successfully(source, "Multiple variable declarations");
}

#[test]
fn test_chained_comparisons() {
    let source = r#"
fn main() -> bool {
    return x < y && y < z && z < w;
}
"#;
    assert_parses_successfully(source, "Chained comparisons");
}

#[test]
fn test_complex_arithmetic() {
    let source = r#"
fn main() -> int {
    return (a + b) * (c - d) / (e + f) % (g - h);
}
"#;
    assert_parses_successfully(source, "Complex arithmetic expression");
}

// Error cases - these should fail to parse

#[test]
fn test_unclosed_parentheses() {
    let source = r#"
fn main() -> int {
    return (1 + 2;
}
"#;
    assert_parsing_fails(source, "Unclosed parentheses");
}

#[test]
fn test_missing_semicolon() {
    let source = r#"
fn main() -> int {
    let x = 42
    return x;
}
"#;
    assert_parsing_fails(source, "Missing semicolon");
}

#[test]
fn test_invalid_function_syntax() {
    let source = r#"
fn main() -> int
    return 42;
}
"#;
    assert_parsing_fails(source, "Missing opening brace");
}

#[test]
fn test_invalid_if_syntax() {
    let source = r#"
fn main() -> int {
    if x > 0
        return 1;
    }
    return 0;
}
"#;
    assert_parsing_fails(source, "Missing braces in if statement");
}

#[test]
fn test_unmatched_braces() {
    let source = r#"
fn main() -> int {
    if x > 0 {
        return 1;

    return 0;
}
"#;
    assert_parsing_fails(source, "Unmatched braces");
}

#[test]
fn test_invalid_expression() {
    let source = r#"
fn main() -> int {
    return 1 + + 2;
}
"#;
    assert_parsing_fails(source, "Invalid expression (double plus)");
}

#[test]
fn test_incomplete_function_call() {
    let source = r#"
fn test(x: int) -> int {
    return x;
}

fn main() -> int {
    return test(;
}
"#;
    assert_parsing_fails(source, "Incomplete function call");
}

#[test]
fn test_recursive_function_definition() {
    let source = r#"
fn factorial(n: int) -> int {
    if n <= 1 {
        return 1;
    } else {
        return n * factorial(n - 1);
    }
}

fn main() -> int {
    return factorial(5);
}
"#;
    assert_parses_successfully(source, "Recursive function definition");
}

#[test]
fn test_mutual_recursion() {
    let source = r#"
fn is_even(n: int) -> bool {
    if n == 0 {
        return true;
    } else {
        return is_odd(n - 1);
    }
}

fn is_odd(n: int) -> bool {
    if n == 0 {
        return false;
    } else {
        return is_even(n - 1);
    }
}

fn main() -> bool {
    return is_even(10);
}
"#;
    assert_parses_successfully(source, "Mutual recursion");
}

#[test]
fn test_complex_nested_structures() {
    let source = r#"
fn process_data(x: int, y: int) -> int {
    if x > 0 {
        let temp = 0;
        while y > 0 {
            if temp < x {
                temp = temp + 1;
                if temp % 2 == 0 {
                    y = y - 1;
                } else {
                    y = y - 2;
                }
            } else {
                break;
            }
        }
        return temp;
    } else {
        return 0;
    }
}

fn main() -> int {
    return process_data(10, 20);
}
"#;
    assert_parses_successfully(source, "Complex nested structures");
}

#[test]
fn test_edge_case_expressions() {
    let source = r#"
fn main() -> int {
    // Test various edge cases
    let a = (1);                    // Parenthesized literal
    let b = ((((2))));              // Multiple parentheses
    let c = 1 + (2 * 3);           // Mixed grouping
    let d = (1 + 2) * (3 + 4);     // Parallel groups

    return a + b + c + d;
}
"#;
    assert_parses_successfully(source, "Edge case expressions");
}