// NEURO Programming Language - Semantic Analysis
// Integration tests: Expression-based (implicit) returns

use semantic_analysis::{type_check, TypeError};

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
