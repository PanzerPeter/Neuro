// NEURO Programming Language - Syntax Parsing Tests
// Error case tests

use syntax_parsing::{parse, parse_expr};

#[test]
fn test_error_unexpected_token() {
    let result = parse_expr("@");
    assert!(result.is_err());
}

#[test]
fn test_error_unclosed_paren() {
    let result = parse_expr("(42");
    assert!(result.is_err());
}

#[test]
fn test_error_missing_function_name() {
    let source = "func () {}";
    let result = parse(source);
    assert!(result.is_err());
}

#[test]
fn test_error_missing_function_params() {
    let source = "func test {}";
    let result = parse(source);
    assert!(result.is_err());
}

#[test]
fn test_error_missing_function_body() {
    let source = "func test()";
    let result = parse(source);
    assert!(result.is_err());
}

#[test]
fn test_error_invalid_parameter_syntax() {
    let source = "func test(x) {}";
    let result = parse(source);
    assert!(result.is_err());
}

#[test]
fn test_error_missing_parameter_type() {
    let source = "func test(x:) {}";
    let result = parse(source);
    assert!(result.is_err());
}

#[test]
fn test_error_trailing_comma_in_params() {
    let source = "func test(x: i32,) {}";
    let result = parse(source);
    assert!(result.is_err());
}

#[test]
fn test_error_unclosed_function_body() {
    let source = "func test() { val x = 1";
    let result = parse(source);
    assert!(result.is_err());
}

#[test]
fn test_error_invalid_statement() {
    let source = r#"
        func test() {
            ;;;
        }
    "#;
    let result = parse(source);
    assert!(result.is_err());
}

#[test]
fn test_error_val_without_name() {
    let source = r#"
        func test() {
            val = 42
        }
    "#;
    let result = parse(source);
    assert!(result.is_err());
}

#[test]
fn test_error_incomplete_if_statement() {
    let source = r#"
        func test() {
            if
        }
    "#;
    let result = parse(source);
    assert!(result.is_err());
}

#[test]
fn test_error_if_without_condition() {
    let source = r#"
        func test() {
            if { val x = 1 }
        }
    "#;
    let result = parse(source);
    assert!(result.is_err());
}

#[test]
fn test_error_if_without_body() {
    let source = r#"
        func test() {
            if true
        }
    "#;
    let result = parse(source);
    assert!(result.is_err());
}

#[test]
fn test_error_else_without_if() {
    let source = r#"
        func test() {
            else { val x = 1 }
        }
    "#;
    let result = parse(source);
    assert!(result.is_err());
}

#[test]
fn test_error_incomplete_binary_expression() {
    let result = parse_expr("2 +");
    assert!(result.is_err());
}

#[test]
fn test_error_incomplete_unary_expression() {
    let result = parse_expr("-");
    assert!(result.is_err());
}

#[test]
fn test_error_empty_function_call() {
    let result = parse_expr("()");
    assert!(result.is_err());
}

#[test]
fn test_error_trailing_comma_in_call() {
    let result = parse_expr("foo(1, 2,)");
    assert!(result.is_err());
}

#[test]
fn test_error_missing_assignment_value() {
    let source = r#"
        func test() {
            mut x = 0
            x =
        }
    "#;
    let result = parse(source);
    assert!(result.is_err());
}

#[test]
fn test_error_assign_to_literal() {
    let source = r#"
        func test() {
            42 = 10
        }
    "#;
    let result = parse(source);
    assert!(result.is_err());
}

#[test]
fn test_error_double_operator() {
    let result = parse_expr("2 ++ 3");
    assert!(result.is_err());
}

#[test]
fn test_error_invalid_type_annotation() {
    let source = r#"
        func test() {
            val x: = 42
        }
    "#;
    let result = parse(source);
    assert!(result.is_err());
}

#[test]
fn test_error_return_type_without_arrow() {
    let source = "func test() i32 {}";
    let result = parse(source);
    assert!(result.is_err());
}

#[test]
fn test_error_missing_return_type_after_arrow() {
    let source = "func test() -> {}";
    let result = parse(source);
    assert!(result.is_err());
}

#[test]
fn test_error_nested_unclosed_parens() {
    let result = parse_expr("((2 + 3)");
    assert!(result.is_err());
}

#[test]
fn test_error_empty_source() {
    let result = parse("");
    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), 0);
}

#[test]
fn test_error_only_whitespace() {
    let result = parse("   \n\n  \n  ");
    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), 0);
}

#[test]
fn test_error_unexpected_eof_in_expression() {
    let result = parse_expr("2 + 3 *");
    assert!(result.is_err());
}

#[test]
fn test_error_max_depth_exceeded() {
    let mut expr = String::from("1");
    for _ in 0..300 {
        expr = format!("({})", expr);
    }
    let result = parse_expr(&expr);
    assert!(result.is_err());
    if let Err(e) = result {
        assert!(e.to_string().contains("maximum expression nesting depth"));
    }
}

#[test]
fn test_error_duplicate_parameter_names() {
    let source = "func test(x: i32, y: i32, x: i32) {}";
    let result = parse(source);
    assert!(result.is_err());
    if let Err(e) = result {
        assert!(e.to_string().contains("duplicate parameter"));
    }
}
