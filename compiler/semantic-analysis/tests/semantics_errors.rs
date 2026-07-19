// Integration tests: General type-checking error cases

use semantic_analysis::{type_check, TypeError};

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
fn error_array_destructure_length_mismatch() {
    // A rest-less array pattern must bind every element. Binding two from a
    // four-element array is an arity error.
    let source = r#"func test() -> i32 {
        val arr: [i32; 4] = [1, 2, 3, 4]
        val [a, b] = arr
        return a + b
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| matches!(e, TypeError::ArrayPatternLengthMismatch { .. })));
}

#[test]
fn error_array_destructure_too_many_before_rest() {
    // A pattern that binds more leading elements than the array holds, even with a
    // rest, is an arity error.
    let source = r#"func test() -> i32 {
        val arr: [i32; 2] = [1, 2]
        val [a, b, c, ..rest] = arr
        return a
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| matches!(e, TypeError::ArrayPatternLengthMismatch { .. })));
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
