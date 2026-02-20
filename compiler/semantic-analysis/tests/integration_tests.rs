// NEURO Programming Language - Semantic Analysis
// Integration tests for type checking complete programs

use semantic_analysis::{type_check, TypeError};

// Integration tests with actual programs
#[test]
fn type_check_simple_function() {
    let source = r#"func add(a: i32, b: i32) -> i32 {
        return a + b
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(
        result.is_ok(),
        "Expected successful type check, got: {:?}",
        result
    );
}

#[test]
fn type_check_function_with_variable() {
    let source = r#"func calculate(x: i32) -> i32 {
        val result: i32 = x * 2
        return result
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(result.is_ok());
}

#[test]
fn type_check_function_call() {
    let source = r#"
        func add(a: i32, b: i32) -> i32 {
            return a + b
        }

        func main() -> i32 {
            val result: i32 = add(5, 3)
            return result
        }
    "#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(result.is_ok());
}

#[test]
fn type_check_if_statement() {
    let source = r#"func test(x: i32) -> i32 {
        if x > 0 {
            return 1
        } else {
            return -1
        }
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    if let Err(ref errors) = result {
        for error in errors {
            eprintln!("Type error: {:?}", error);
        }
    }
    assert!(result.is_ok());
}

#[test]
fn type_check_boolean_operators() {
    let source = r#"func test(a: bool, b: bool) -> bool {
        return a && b || !a
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(result.is_ok());
}

#[test]
fn type_check_nested_scopes() {
    let source = r#"func test() -> i32 {
        val x: i32 = 1
        if true {
            val y: i32 = 2
            return x + y
        }
        return x
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(result.is_ok());
}

#[test]
fn type_check_while_statement() {
    let source = r#"func test() -> i32 {
        mut i: i32 = 0
        while i < 5 {
            i = i + 1
        }
        return i
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(result.is_ok());
}

#[test]
fn type_check_variable_shadowing() {
    let source = r#"func test() -> i32 {
        val x: i32 = 1
        if true {
            val x: i32 = 2
            return x
        }
        return x
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(result.is_ok());
}

#[test]
fn type_check_extended_integers_i8() {
    let source = r#"func test(a: i8, b: i8) -> i8 {
        return a + b
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(
        result.is_ok(),
        "i8 arithmetic should type check: {:?}",
        result
    );
}

#[test]
fn type_check_extended_integers_i16() {
    let source = r#"func test(a: i16, b: i16) -> i16 {
        val result: i16 = a * b
        return result
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(
        result.is_ok(),
        "i16 arithmetic should type check: {:?}",
        result
    );
}

#[test]
fn type_check_extended_integers_u8() {
    let source = r#"func test(x: u8) -> u8 {
        return x + x
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(
        result.is_ok(),
        "u8 arithmetic should type check: {:?}",
        result
    );
}

#[test]
fn type_check_extended_integers_u16() {
    let source = r#"func test(a: u16, b: u16) -> u16 {
        return a - b
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(
        result.is_ok(),
        "u16 arithmetic should type check: {:?}",
        result
    );
}

#[test]
fn type_check_extended_integers_u32() {
    let source = r#"func test(a: u32, b: u32) -> u32 {
        return a / b
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(
        result.is_ok(),
        "u32 arithmetic should type check: {:?}",
        result
    );
}

#[test]
fn type_check_extended_integers_u64() {
    let source = r#"func test(a: u64, b: u64) -> bool {
        return a > b
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(
        result.is_ok(),
        "u64 comparison should type check: {:?}",
        result
    );
}

// Error cases
#[test]
fn error_undefined_variable() {
    let source = r#"func test() -> i32 {
        return undefined_var
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert_eq!(errors.len(), 1);
    assert!(matches!(errors[0], TypeError::UndefinedVariable { .. }));
}

#[test]
fn error_type_mismatch() {
    let source = r#"func test() -> i32 {
        val x: i32 = true
        return x
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| matches!(e, TypeError::Mismatch { .. })));
}

#[test]
fn error_wrong_operator_type() {
    let source = r#"func test() -> i32 {
        return true + false
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| matches!(e, TypeError::InvalidBinaryOperator { .. })));
}

#[test]
fn error_return_type_mismatch() {
    let source = r#"func test() -> i32 {
        return true
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| matches!(e, TypeError::ReturnTypeMismatch { .. })));
}

#[test]
fn error_argument_count_mismatch() {
    let source = r#"
        func add(a: i32, b: i32) -> i32 {
            return a + b
        }

        func main() -> i32 {
            return add(5)
        }
    "#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| matches!(e, TypeError::ArgumentCountMismatch { .. })));
}

#[test]
fn error_argument_type_mismatch() {
    let source = r#"
        func add(a: i32, b: i32) -> i32 {
            return a + b
        }

        func main() -> i32 {
            return add(5, true)
        }
    "#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| matches!(e, TypeError::Mismatch { .. })));
}

#[test]
fn error_undefined_function() {
    let source = r#"func main() -> i32 {
        return undefined_func()
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| matches!(e, TypeError::UndefinedFunction { .. })));
}

#[test]
fn error_if_condition_not_bool() {
    let source = r#"func test() -> i32 {
        if 42 {
            return 1
        }
        return 0
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| matches!(e, TypeError::Mismatch { .. })));
}

#[test]
fn error_while_condition_not_bool() {
    let source = r#"func test() -> i32 {
        mut i: i32 = 0
        while 42 {
            i = i + 1
        }
        return i
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| matches!(e, TypeError::Mismatch { .. })));
}

#[test]
fn error_duplicate_variable() {
    let source = r#"func test() -> i32 {
        val x: i32 = 1
        val x: i32 = 2
        return x
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| matches!(e, TypeError::VariableAlreadyDefined { .. })));
}

#[test]
fn error_duplicate_function() {
    let source = r#"
        func test() -> i32 {
            return 1
        }

        func test() -> i32 {
            return 2
        }
    "#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| matches!(e, TypeError::FunctionAlreadyDefined { .. })));
}

#[test]
fn error_unknown_type_name() {
    let source = r#"func test(x: unknown_type) -> i32 {
        return 0
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| matches!(e, TypeError::UnknownTypeName { .. })));
}

#[test]
fn error_signed_unsigned_mismatch() {
    let source = r#"func test(a: i32, b: u32) -> i32 {
        return a + b
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(result.is_err(), "i32 + u32 should fail type check");
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| matches!(e, TypeError::Mismatch { .. })));
}

#[test]
fn error_different_width_mismatch() {
    let source = r#"func test(a: i8, b: i16) -> i8 {
        return a + b
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(result.is_err(), "i8 + i16 should fail type check");
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| matches!(e, TypeError::Mismatch { .. })));
}

#[test]
fn error_unsigned_with_float() {
    let source = r#"func test(a: u32, b: f32) -> u32 {
        return a + b
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(result.is_err(), "u32 + f32 should fail type check");
}

// Milestone test: The example from roadmap.md
#[test]
fn type_check_milestone_program() {
    let source = r#"
        func add(a: i32, b: i32) -> i32 {
            return a + b
        }

        func main() -> i32 {
            val result: i32 = add(5, 3)
            return result
        }
    "#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(
        result.is_ok(),
        "Milestone program should type check successfully, got: {:?}",
        result
    );
}

// ==================================================
// Expression-Based Returns Tests (Phase 1 Feature)
// ==================================================

#[test]
fn expression_based_return_simple_literal() {
    let source = r#"func get_number() -> i32 {
        42
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(
        result.is_ok(),
        "Simple literal expression should work as return"
    );
}

#[test]
fn expression_based_return_arithmetic() {
    let source = r#"func add(a: i32, b: i32) -> i32 {
        a + b
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(
        result.is_ok(),
        "Arithmetic expression should work as return"
    );
}

#[test]
fn expression_based_return_variable() {
    let source = r#"func get_value(x: i32) -> i32 {
        val result: i32 = x * 2
        result
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(result.is_ok(), "Variable expression should work as return");
}

#[test]
fn expression_based_return_function_call() {
    let source = r#"
        func double(x: i32) -> i32 {
            x * 2
        }

        func quad(x: i32) -> i32 {
            double(x) + double(x)
        }
    "#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(
        result.is_ok(),
        "Function call expression should work as return"
    );
}

#[test]
fn expression_based_return_comparison() {
    let source = r#"func is_positive(x: i32) -> bool {
        x > 0
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(
        result.is_ok(),
        "Comparison expression should work as return"
    );
}

#[test]
fn expression_based_return_logical_expression() {
    let source = r#"func and_op(a: bool, b: bool) -> bool {
        a && b
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(result.is_ok(), "Logical expression should work as return");
}

#[test]
fn expression_based_return_with_statements_before() {
    let source = r#"func compute(x: i32, y: i32) -> i32 {
        val a: i32 = x + y
        val b: i32 = x * y
        a + b
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(
        result.is_ok(),
        "Trailing expression after statements should work"
    );
}

#[test]
fn expression_based_return_wrong_type() {
    let source = r#"func get_bool() -> bool {
        42
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(
        result.is_err(),
        "Wrong type for implicit return should fail"
    );
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| matches!(e, TypeError::ReturnTypeMismatch { .. })));
}

#[test]
fn expression_based_return_type_compatibility() {
    let source = r#"func get_number() -> i32 {
        1 + 2
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(
        result.is_ok(),
        "Inferred i32 should be compatible with declared i32"
    );
}

#[test]
fn mixed_explicit_and_implicit_returns() {
    let source = r#"
        func explicit_return(x: i32) -> i32 {
            return x + 1
        }

        func implicit_return(x: i32) -> i32 {
            x + 1
        }
    "#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(
        result.is_ok(),
        "Both explicit and implicit returns should coexist"
    );
}

#[test]
fn expression_based_return_extended_types() {
    // Note: Literals default to i32/f64, so we test with i32 and f64 directly
    // Type inference for other literal types is a deferred Phase 1 feature
    let source = r#"
        func get_i32(x: i32) -> i32 {
            x
        }

        func get_f64(y: f64) -> f64 {
            y
        }

        func compute_i32() -> i32 {
            1 + 2
        }
    "#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(
        result.is_ok(),
        "Extended types should work with implicit returns"
    );
}

// ============================================================================
// String Type Tests (Phase 1)
// ============================================================================

#[test]
fn type_check_string_literal() {
    let source = r#"func get_message() -> string {
        return "Hello, NEURO!"
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(
        result.is_ok(),
        "String literal should type-check as string type"
    );
}

#[test]
fn type_check_string_variable() {
    let source = r#"func greet() -> string {
        val message: string = "Hello"
        return message
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(result.is_ok(), "String variable declaration should work");
}

#[test]
fn type_check_string_parameter() {
    let source = r#"func print_message(msg: string) -> string {
        return msg
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(
        result.is_ok(),
        "String function parameter should type-check correctly"
    );
}

#[test]
fn type_check_string_with_escapes() {
    let source = r#"func get_escaped_string() -> string {
        return "Hello\nWorld\t!"
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(
        result.is_ok(),
        "String literals with escape sequences should work"
    );
}

#[test]
fn type_check_string_empty() {
    let source = r#"func get_empty() -> string {
        return ""
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(result.is_ok(), "Empty string literal should work");
}

#[test]
fn type_check_string_mismatch_with_integer() {
    let source = r#"func wrong() -> string {
        return 42
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(
        result.is_err(),
        "Returning integer when string expected should fail"
    );
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| matches!(e, TypeError::ReturnTypeMismatch { .. })));
}

#[test]
fn type_check_string_mismatch_with_bool() {
    let source = r#"func wrong_bool() -> string {
        return true
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(
        result.is_err(),
        "Returning bool when string expected should fail"
    );
}

#[test]
fn type_check_string_variable_type_mismatch() {
    let source = r#"func wrong_var() -> i32 {
        val msg: string = "hello"
        return msg
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(
        result.is_err(),
        "Returning string when i32 expected should fail"
    );
}

#[test]
fn type_check_string_implicit_return() {
    let source = r#"func implicit_string() -> string {
        "Hello, implicit return!"
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(result.is_ok(), "String with implicit return should work");
}

#[test]
fn type_check_multiple_string_functions() {
    let source = r#"
        func get_greeting() -> string {
            return "Hello"
        }

        func get_name() -> string {
            return "NEURO"
        }

        func use_strings() -> string {
            val g: string = get_greeting()
            val n: string = get_name()
            return g
        }
    "#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(
        result.is_ok(),
        "Multiple string functions should type-check correctly"
    );
}
