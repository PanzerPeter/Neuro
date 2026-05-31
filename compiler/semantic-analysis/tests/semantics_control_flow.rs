// NEURO Programming Language - Semantic Analysis
// Integration tests: Control flow: if / while / break / continue

use semantic_analysis::{type_check, TypeError};

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
fn type_check_break_inside_while_loop() {
    let source = r#"func test() -> i32 {
        mut i: i32 = 0
        while true {
            if i == 3 {
                break
            }
            i = i + 1
        }
        return i
    }"#;

    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(result.is_ok(), "Expected break in loop to type check");
}

#[test]
fn type_check_continue_inside_while_loop() {
    let source = r#"func test() -> i32 {
        mut i: i32 = 0
        mut sum: i32 = 0

        while i < 5 {
            i = i + 1
            if i == 3 {
                continue
            }
            sum = sum + i
        }

        return sum
    }"#;

    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(result.is_ok(), "Expected continue in loop to type check");
}

#[test]
fn error_break_outside_loop() {
    let source = r#"func test() -> i32 {
        break
        return 0
    }"#;

    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| matches!(e, TypeError::BreakOutsideLoop { .. })));
}

#[test]
fn error_continue_outside_loop() {
    let source = r#"func test() -> i32 {
        continue
        return 0
    }"#;

    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| matches!(e, TypeError::ContinueOutsideLoop { .. })));
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
