//! Tests for semantic analysis error handling
//! These tests ensure proper error reporting for various semantic violations

use semantic_analysis::{analyze_program, SemanticError};
use lexical_analysis::Lexer;
use syntax_parsing::Parser;

/// Helper function to parse source code and run semantic analysis
fn analyze_source(source: &str) -> Result<semantic_analysis::SemanticInfo, SemanticError> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Lexical analysis should succeed");

    let mut parser = Parser::new(tokens);
    let ast = parser.parse().expect("Parsing should succeed");

    analyze_program(&ast)
}

/// Helper function to assert that source code produces a semantic error
fn assert_semantic_error(source: &str, expected_error_pattern: &str) {
    match analyze_source(source) {
        Err(error) => {
            let error_message = error.to_string();
            assert!(
                error_message.contains(expected_error_pattern),
                "Expected error containing '{}', but got: '{}'",
                expected_error_pattern,
                error_message
            );
        }
        Ok(_) => panic!("Expected semantic error, but analysis succeeded"),
    }
}

#[test]
fn test_undefined_variable_error() {
    let source = r#"
fn main() -> int {
    return undefined_var;
}
"#;
    assert_semantic_error(source, "undefined");
}

#[test]
fn test_type_mismatch_return() {
    let source = r#"
fn main() -> int {
    return true;
}
"#;
    assert_semantic_error(source, "type");
}

#[test]
fn test_undefined_function_call() {
    let source = r#"
fn main() -> int {
    return nonexistent_function();
}
"#;
    assert_semantic_error(source, "undefined");
}

#[test]
fn test_wrong_argument_count() {
    let source = r#"
fn add(x: int, y: int) -> int {
    return x + y;
}

fn main() -> int {
    return add(1, 2, 3);
}
"#;
    assert_semantic_error(source, "argument");
}

#[test]
fn test_wrong_argument_type() {
    let source = r#"
fn add(x: int, y: int) -> int {
    return x + y;
}

fn main() -> int {
    return add(1, true);
}
"#;
    assert_semantic_error(source, "type");
}

#[test]
fn test_duplicate_function_definition() {
    let source = r#"
fn test() -> int {
    return 1;
}

fn test() -> int {
    return 2;
}

fn main() -> int {
    return test();
}
"#;
    assert_semantic_error(source, "duplicate");
}

#[test]
fn test_duplicate_parameter_names() {
    let source = r#"
fn test(x: int, x: int) -> int {
    return x;
}

fn main() -> int {
    return test(1, 2);
}
"#;
    assert_semantic_error(source, "duplicate");
}

#[test]
fn test_missing_return_statement() {
    let source = r#"
fn test() -> int {
    let x = 42;
}

fn main() -> int {
    return test();
}
"#;
    assert_semantic_error(source, "return");
}

#[test]
fn test_unreachable_code_after_return() {
    let source = r#"
fn test() -> int {
    return 42;
    let x = 1;
}

fn main() -> int {
    return test();
}
"#;
    assert_semantic_error(source, "unreachable");
}

#[test]
fn test_invalid_binary_operation() {
    let source = r#"
fn main() -> bool {
    return true + false;
}
"#;
    assert_semantic_error(source, "operator");
}

#[test]
fn test_void_function_return_value() {
    let source = r#"
fn void_func() {
    // No return
}

fn main() -> int {
    return void_func();
}
"#;
    assert_semantic_error(source, "void");
}

#[test]
fn test_recursive_function_type_checking() {
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
    // This should pass semantic analysis
    let result = analyze_source(source);
    assert!(result.is_ok(), "Recursive function should pass semantic analysis");
}

#[test]
fn test_complex_expression_type_checking() {
    let source = r#"
fn main() -> bool {
    let x = 10;
    let y = 20;
    return (x + y) > (x * 2) && x < y;
}
"#;
    // This should pass semantic analysis
    let result = analyze_source(source);
    assert!(result.is_ok(), "Complex expression should pass semantic analysis");
}

#[test]
fn test_nested_function_calls() {
    let source = r#"
fn double(x: int) -> int {
    return x * 2;
}

fn quadruple(x: int) -> int {
    return double(double(x));
}

fn main() -> int {
    return quadruple(5);
}
"#;
    // This should pass semantic analysis
    let result = analyze_source(source);
    assert!(result.is_ok(), "Nested function calls should pass semantic analysis");
}

#[test]
fn test_variable_shadowing() {
    let source = r#"
fn main() -> int {
    let x = 10;
    if true {
        let x = 20;
        return x;
    }
    return x;
}
"#;
    // This should pass semantic analysis (variable shadowing is allowed)
    let result = analyze_source(source);
    assert!(result.is_ok(), "Variable shadowing should be allowed");
}