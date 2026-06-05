// Neuro Programming Language - Semantic Analysis
// Integration tests: String type

use semantic_analysis::{type_check, TypeError};

#[test]
fn type_check_string_literal() {
    let source = r#"func get_message() -> string {
        return "Hello, Neuro!"
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
            return "Neuro"
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
