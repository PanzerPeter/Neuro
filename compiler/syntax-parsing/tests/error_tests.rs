// Error case tests

use syntax_parsing::{parse, parse_expr};

fn parse_error(source: &str) -> String {
    parse(source)
        .expect_err("source should be rejected")
        .to_string()
}

fn expr_error(source: &str) -> String {
    parse_expr(source)
        .expect_err("expression should be rejected")
        .to_string()
}

#[test]
fn test_error_unexpected_token() {
    let err = expr_error("@");
    assert!(
        err.contains("unexpected token At, expected expression"),
        "{err}"
    );
}

#[test]
fn test_error_unclosed_paren() {
    let err = expr_error("(42");
    assert!(err.contains("unexpected token Eof, expected ')'"), "{err}");
}

#[test]
fn test_error_missing_function_name() {
    let source = "func () {}";
    let err = parse_error(source);
    assert!(err.contains("expected function name"), "{err}");
}

#[test]
fn test_error_missing_function_params() {
    let source = "func test {}";
    let err = parse_error(source);
    assert!(
        err.contains("unexpected token LeftBrace, expected '('"),
        "{err}"
    );
}

#[test]
fn test_error_missing_function_body() {
    let source = "func test()";
    let err = parse_error(source);
    assert!(err.contains("unexpected token Eof, expected '{'"), "{err}");
}

#[test]
fn test_error_invalid_parameter_syntax() {
    let source = "func test(x) {}";
    let err = parse_error(source);
    assert!(
        err.contains("unexpected token RightParen, expected ':'"),
        "{err}"
    );
}

#[test]
fn test_error_missing_parameter_type() {
    let source = "func test(x:) {}";
    let err = parse_error(source);
    assert!(
        err.contains("unexpected token RightParen, expected type name"),
        "{err}"
    );
}

#[test]
fn test_error_trailing_comma_in_params() {
    let source = "func test(x: i32,) {}";
    let err = parse_error(source);
    assert!(
        err.contains("unexpected token RightParen, expected parameter name"),
        "{err}"
    );
}

#[test]
fn test_error_unclosed_function_body() {
    let source = "func test() { val x = 1";
    let err = parse_error(source);
    assert!(err.contains("unexpected token Eof, expected '}'"), "{err}");
}

#[test]
fn test_error_invalid_statement() {
    let source = r#"
        func test() {
            ;;;
        }
    "#;
    let err = parse_error(source);
    assert!(err.contains("unexpected token Semicolon"), "{err}");
}

#[test]
fn test_error_val_without_name() {
    let source = r#"
        func test() {
            val = 42
        }
    "#;
    let err = parse_error(source);
    assert!(
        err.contains("unexpected token Equal, expected variable name"),
        "{err}"
    );
}

#[test]
fn test_error_incomplete_if_statement() {
    let source = r#"
        func test() {
            if
        }
    "#;
    let err = parse_error(source);
    assert!(
        err.contains("unexpected token RightBrace, expected expression"),
        "{err}"
    );
}

#[test]
fn test_error_if_without_condition() {
    // The braces parse as a block-expression condition, so it is the body that is missing.
    let source = r#"
        func test() {
            if { val x = 1 }
        }
    "#;
    let err = parse_error(source);
    assert!(
        err.contains("unexpected token RightBrace, expected '{'"),
        "{err}"
    );
}

#[test]
fn test_error_if_without_body() {
    let source = r#"
        func test() {
            if true
        }
    "#;
    let err = parse_error(source);
    assert!(
        err.contains("unexpected token RightBrace, expected '{'"),
        "{err}"
    );
}

#[test]
fn test_error_else_without_if() {
    let source = r#"
        func test() {
            else { val x = 1 }
        }
    "#;
    let err = parse_error(source);
    assert!(err.contains("unexpected token Else"), "{err}");
}

#[test]
fn test_error_incomplete_binary_expression() {
    let err = expr_error("2 +");
    assert!(
        err.contains("unexpected end of file, expected expression"),
        "{err}"
    );
}

#[test]
fn test_error_incomplete_unary_expression() {
    let err = expr_error("-");
    assert!(
        err.contains("unexpected end of file, expected expression"),
        "{err}"
    );
}

#[test]
fn test_error_empty_parens_are_not_an_expression() {
    let err = expr_error("()");
    assert!(
        err.contains("unexpected token RightParen, expected expression"),
        "{err}"
    );
}

#[test]
fn test_error_trailing_comma_in_call() {
    let err = expr_error("foo(1, 2,)");
    assert!(
        err.contains("unexpected token RightParen, expected expression"),
        "{err}"
    );
}

#[test]
fn test_error_missing_assignment_value() {
    let source = r#"
        func test() {
            mut x = 0
            x =
        }
    "#;
    let err = parse_error(source);
    assert!(
        err.contains("unexpected token RightBrace, expected expression"),
        "{err}"
    );
}

#[test]
fn test_error_assign_to_literal() {
    let source = r#"
        func test() {
            42 = 10
        }
    "#;
    let err = parse_error(source);
    assert!(err.contains("the left of `=` must be a place"), "{err}");
}

#[test]
fn test_error_double_operator() {
    let err = expr_error("2 ++ 3");
    assert!(
        err.contains("unexpected token Plus, expected expression"),
        "{err}"
    );
}

#[test]
fn test_error_invalid_type_annotation() {
    let source = r#"
        func test() {
            val x: = 42
        }
    "#;
    let err = parse_error(source);
    assert!(
        err.contains("unexpected token Equal, expected type name"),
        "{err}"
    );
}

#[test]
fn test_error_return_type_without_arrow() {
    let source = "func test() i32 {}";
    let err = parse_error(source);
    assert!(
        err.contains("unexpected token Identifier(\"i32\"), expected '{'"),
        "{err}"
    );
}

#[test]
fn test_error_missing_return_type_after_arrow() {
    let source = "func test() -> {}";
    let err = parse_error(source);
    assert!(
        err.contains("unexpected token LeftBrace, expected type name"),
        "{err}"
    );
}

#[test]
fn test_error_nested_unclosed_parens() {
    let err = expr_error("((2 + 3)");
    assert!(err.contains("unexpected token Eof, expected ')'"), "{err}");
}

#[test]
fn test_empty_source_has_no_items() {
    assert_eq!(parse("").expect("empty source parses").len(), 0);
}

#[test]
fn test_whitespace_only_source_has_no_items() {
    let items = parse("   \n\n  \n  ").expect("whitespace parses");
    assert_eq!(items.len(), 0);
}

#[test]
fn test_error_unexpected_eof_in_expression() {
    let err = expr_error("2 + 3 *");
    assert!(
        err.contains("unexpected end of file, expected expression"),
        "{err}"
    );
}

#[test]
fn test_error_max_depth_exceeded() {
    let mut expr = String::from("1");
    for _ in 0..300 {
        expr = format!("({})", expr);
    }
    let err = expr_error(&expr);
    assert!(err.contains("maximum expression nesting depth"), "{err}");
}

#[test]
fn test_error_duplicate_parameter_names() {
    let source = "func test(x: i32, y: i32, x: i32) {}";
    let err = parse_error(source);
    assert!(err.contains("duplicate parameter name 'x'"), "{err}");
}

// Neuro statements are newline-terminated; the language has NO semicolons.
// A trailing `;` is an unexpected token, not a no-op. These tests lock in
// that decision so it stays consistent with the docs (which tell users not
// to write semicolons).
#[test]
fn test_error_semicolon_after_binding() {
    let source = r#"
        func test() {
            val x: i32 = 10;
        }
    "#;
    let err = parse_error(source);
    assert!(err.contains("unexpected token Semicolon"), "{err}");
}

#[test]
fn test_error_semicolon_after_expression() {
    let source = r#"
        func test() -> i32 {
            val x: i32 = 10
            x;
        }
    "#;
    let err = parse_error(source);
    assert!(err.contains("unexpected token Semicolon"), "{err}");
}

#[test]
fn test_error_semicolon_after_return() {
    let source = r#"
        func test() -> i32 {
            return 1;
        }
    "#;
    let err = parse_error(source);
    assert!(err.contains("unexpected token Semicolon"), "{err}");
}
