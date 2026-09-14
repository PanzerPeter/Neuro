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

#[test]
fn error_unmatchable_scrutinee_reports_once() {
    // A `string` scrutinee is rejected up front. The literal patterns must not
    // then be re-checked against it: doing so produced a second, nonsensical
    // "this pattern matches a `string` but the value has type string".
    let source = r#"func test() -> i32 {
        val s: string = "a"
        return match s {
            "a" => 1,
            _ => 0,
        }
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert_eq!(errors.len(), 1);
    assert!(matches!(
        errors[0],
        TypeError::UnsupportedMatchScrutinee { .. }
    ));
}

#[test]
fn regression_failed_binding_does_not_cascade_into_undefined_variable() {
    // A binding whose initializer does not type-check is still bound, at
    // `Type::Unknown`, exactly as a parameter whose type failed to resolve is.
    // Leaving the name undefined turned every later use into a second,
    // misleading `UndefinedVariable` report chasing an error already given.
    let source = r#"func test() -> i32 {
        val a: i32 = 1
        val b: bool = true
        val c = a + b
        return c + c
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(
        !errors
            .iter()
            .any(|e| matches!(e, TypeError::UndefinedVariable { .. })),
        "the failed binding cascaded into an undefined-variable report: {errors:?}"
    );
    assert_eq!(
        errors.len(),
        1,
        "expected only the initializer's own error: {errors:?}"
    );
}

#[test]
fn regression_diverging_initializer_is_rejected_as_a_valueless_binding() {
    // `panic(...)` types as `Type::Unknown` because it diverges, not because an
    // error was reported for it. Binding it produced a name with no value, which
    // the checker passed and codegen could only answer as an internal error.
    let source = r#"func test() -> i32 {
        val x = panic("boom")
        return 0
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(result.is_err(), "a valueless binding must not type-check");
    let errors = result.unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::VoidBinding { .. })),
        "expected a VoidBinding diagnostic, got: {errors:?}"
    );
}
