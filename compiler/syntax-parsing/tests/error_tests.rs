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
    assert!(err.contains("maximum nesting depth"), "{err}");
}

/// Parse `source` on a thread with the 8 MiB stack `neurc`'s main thread has, so a
/// nesting form the depth limit misses shows up as a stack overflow (the test binary
/// aborts) rather than passing on a larger stack or failing on the harness's smaller one.
fn parse_error_on_main_sized_stack(source: String) -> String {
    std::thread::Builder::new()
        .stack_size(8 << 20)
        .spawn(move || parse_error(&source))
        .expect("thread spawns")
        .join()
        .expect("parse returns")
}

#[test]
fn test_error_max_depth_covers_every_nesting_form() {
    // Statements, types, patterns and module blocks recurse without passing through
    // the expression parser, so each needs the same limit; before it they overflowed
    // the stack a few hundred levels in.
    let n = 2000;
    let cases = [
        format!(
            "func main() {{ {}{} }}",
            "if true { ".repeat(n),
            "}".repeat(n)
        ),
        format!(
            "func main() {{ {}{} }}",
            "for i in 0..1 { ".repeat(n),
            "}".repeat(n)
        ),
        format!("func f(x: {}i32{}) {{}}", "[".repeat(n), "]".repeat(n)),
        format!("func f(x: {}i32{}) {{}}", "Vec<".repeat(n), ">".repeat(n)),
        format!("func f(x: {}i32) {{}}", "& ".repeat(n)),
        format!("func f(x: {}i32) {{}}", "() -> ".repeat(n)),
        format!(
            "func main() {{ match x {{ {}1{} => 1 }} }}",
            "Some(".repeat(n),
            ")".repeat(n)
        ),
        format!(
            "func main() {{ val {}a, b{} = x }}",
            "(".repeat(n),
            ")".repeat(n)
        ),
        format!("{}{}", "module a { ".repeat(n), "}".repeat(n)),
    ];
    for source in cases {
        let head: String = source.chars().take(24).collect();
        let err = parse_error_on_main_sized_stack(source);
        assert!(err.contains("maximum nesting depth"), "{head}: {err}");
    }
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

#[test]
fn test_error_expression_juxtaposed_on_one_line() {
    // A statement ends at a newline or a closing brace. Two expressions side by side
    // used to parse as two statements, so `val x = 5 6` bound 5 and dropped the 6, and
    // a malformed literal the lexer splits (`0b102` lexes as `0b10` then `2`) bound
    // its first half.
    for source in [
        "func main() -> i32 {\n    val x = 5 6\n    return x\n}",
        "func main() -> i32 {\n    val x = 0b102\n    return x\n}",
        "func main() -> i32 {\n    return 1 2\n}",
        "func main() {\n    f() g()\n}",
    ] {
        let err = parse_error(source);
        assert!(
            err.contains("expected a newline or '}' to end the statement"),
            "{source}: {err}"
        );
    }
}

#[test]
fn test_statements_that_share_a_line_with_their_block_still_parse() {
    for source in [
        "func main() -> i32 { val x = 1\n return x }",
        "func main() -> i32 { if true { return 1 } else { return 2 } }",
        "func main() -> i32 {\n    if true { val a = 1 }\n    return 0\n}",
        "func main() -> i32 {\n    val y = { val a = 1\n a }\n    return y\n}",
        "func main() -> i32 {\n    match 1 { 1 => return 2, _ => return 3 }\n}",
        "func main() -> i32 {\n    val Some(v) = o else { return 0 }\n    return v\n}",
        "func main() -> i32 {\n    for i in 0..3 { }\n    loop { break }\n    return 0\n}",
    ] {
        parse(source).unwrap_or_else(|e| panic!("{source}: {e}"));
    }
}

#[test]
fn test_lex_error_inside_an_interpolation_hole_points_at_the_file() {
    // A hole is re-lexed on its own, so the lexer reports an offset into the hole; the
    // parser has to shift it onto the file the way it shifts the hole's tokens.
    let source = "func main() {\n    print(\"{1 \u{20ac} 2}\")\n}";
    let err = parse(source).expect_err("the euro sign is not a token");
    let at = source.find('\u{20ac}').expect("source holds the character");
    assert_eq!(err.span().map(|s| s.start), Some(at), "{err}");
}
