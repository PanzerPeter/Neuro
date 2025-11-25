// NEURO Programming Language - Syntax Parsing Tests
// Function parsing tests

use syntax_parsing::{parse, Item};

#[test]
fn test_parse_empty_function() {
    let source = "func empty() {}";
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
    let items = result.unwrap();
    assert_eq!(items.len(), 1);
    match &items[0] {
        Item::Function(func) => {
            assert_eq!(func.name.name, "empty");
            assert_eq!(func.params.len(), 0);
            assert!(func.return_type.is_none());
            assert_eq!(func.body.len(), 0);
        }
    }
}

#[test]
fn test_parse_function_with_return_type() {
    let source = "func get_number() -> i32 { return 42 }";
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
    let items = result.unwrap();
    assert_eq!(items.len(), 1);
    match &items[0] {
        Item::Function(func) => {
            assert_eq!(func.name.name, "get_number");
            assert!(func.return_type.is_some());
        }
    }
}

#[test]
fn test_parse_function_with_one_parameter() {
    let source = "func increment(x: i32) -> i32 { return x + 1 }";
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
    let items = result.unwrap();
    match &items[0] {
        Item::Function(func) => {
            assert_eq!(func.params.len(), 1);
            assert_eq!(func.params[0].name.name, "x");
        }
    }
}

#[test]
fn test_parse_function_with_multiple_parameters() {
    let source = "func add(a: i32, b: i32) -> i32 { return a + b }";
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
    let items = result.unwrap();
    match &items[0] {
        Item::Function(func) => {
            assert_eq!(func.params.len(), 2);
            assert_eq!(func.params[0].name.name, "a");
            assert_eq!(func.params[1].name.name, "b");
        }
    }
}

#[test]
fn test_parse_function_with_many_parameters() {
    let source = "func many(a: i32, b: i32, c: i32, d: i32, e: i32) -> i32 { return a }";
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
    let items = result.unwrap();
    match &items[0] {
        Item::Function(func) => {
            assert_eq!(func.params.len(), 5);
        }
    }
}

#[test]
fn test_parse_function_with_body() {
    let source = r#"
        func compute(x: i32) -> i32 {
            val doubled = x * 2
            val result = doubled + 10
            return result
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
    let items = result.unwrap();
    match &items[0] {
        Item::Function(func) => {
            assert_eq!(func.body.len(), 3);
        }
    }
}

#[test]
fn test_parse_function_with_control_flow() {
    let source = r#"
        func abs(x: i32) -> i32 {
            if x < 0 {
                return -x
            } else {
                return x
            }
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_parse_multiple_functions() {
    let source = r#"
        func first() -> i32 {
            return 1
        }

        func second() -> i32 {
            return 2
        }

        func third() -> i32 {
            return 3
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
    let items = result.unwrap();
    assert_eq!(items.len(), 3);
}

#[test]
fn test_parse_function_with_newlines_in_params() {
    let source = r#"
        func multi_param(
            a: i32,
            b: i32,
            c: i32
        ) -> i32 {
            return a + b + c
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_parse_function_calling_another() {
    let source = r#"
        func helper() -> i32 {
            return 42
        }

        func main() -> i32 {
            return helper()
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_parse_function_with_complex_body() {
    let source = r#"
        func factorial(n: i32) -> i32 {
            if n <= 1 {
                return 1
            } else {
                val n_minus_1 = n - 1
                val recursive = factorial(n_minus_1)
                return n * recursive
            }
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_parse_function_with_string_param() {
    let source = r#"
        func greet(name: String) {
            val message = "Hello"
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_parse_function_with_bool_param() {
    let source = r#"
        func check(flag: bool) -> i32 {
            if flag {
                return 1
            } else {
                return 0
            }
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_parse_function_no_return_type() {
    let source = r#"
        func print_hello() {
            val msg = "Hello"
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_parse_function_with_expression_statement() {
    let source = r#"
        func caller() {
            do_something()
            do_something_else()
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_parse_function_long_name() {
    let source = r#"
        func very_long_function_name_with_many_words() {
            val x = 1
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}
