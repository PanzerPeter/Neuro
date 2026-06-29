// Integration tests: Functions, variables, scopes

use semantic_analysis::type_check;

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
