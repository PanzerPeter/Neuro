// NEURO Programming Language - Syntax Parsing Tests
// Statement parsing tests

use syntax_parsing::parse;

#[test]
fn test_parse_val_declaration_with_type_and_init() {
    let source = r#"
        func test() {
            val x: i32 = 42
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_parse_val_declaration_with_init_no_type() {
    let source = r#"
        func test() {
            val x = 42
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_parse_val_declaration_with_type_no_init() {
    let source = r#"
        func test() {
            val x: i32
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_parse_val_declaration_no_type_no_init() {
    let source = r#"
        func test() {
            val x
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_parse_mut_declaration() {
    let source = r#"
        func test() {
            mut counter: i32 = 0
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_parse_assignment_statement() {
    let source = r#"
        func test() {
            mut x: i32 = 0
            x = 42
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_parse_assignment_with_expression() {
    let source = r#"
        func test() {
            mut x: i32 = 0
            x = 2 + 3 * 4
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_parse_return_with_value() {
    let source = r#"
        func test() -> i32 {
            return 42
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_parse_return_without_value() {
    let source = r#"
        func test() {
            return
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_parse_return_with_expression() {
    let source = r#"
        func test() -> i32 {
            return 2 + 3
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_parse_if_statement() {
    let source = r#"
        func test() {
            if true {
                val x = 1
            }
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_parse_if_else_statement() {
    let source = r#"
        func test() {
            if true {
                val x = 1
            } else {
                val x = 2
            }
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_parse_if_else_if_statement() {
    let source = r#"
        func test() {
            if x < 0 {
                val sign = -1
            } else if x > 0 {
                val sign = 1
            } else {
                val sign = 0
            }
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_parse_multiple_else_if() {
    let source = r#"
        func test() {
            if x == 1 {
                val a = 1
            } else if x == 2 {
                val a = 2
            } else if x == 3 {
                val a = 3
            } else {
                val a = 0
            }
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_parse_nested_if_statements() {
    let source = r#"
        func test() {
            if x > 0 {
                if y > 0 {
                    val result = 1
                }
            }
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_parse_expression_statement() {
    let source = r#"
        func test() {
            foo()
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_parse_multiple_statements() {
    let source = r#"
        func test() {
            val x = 1
            val y = 2
            val z = x + y
            return z
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_parse_statements_with_newlines() {
    let source = r#"
        func test() {

            val x = 1


            val y = 2

            return x + y
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_parse_complex_if_condition() {
    let source = r#"
        func test() {
            if x > 0 && y < 10 || z == 5 {
                val result = true
            }
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_parse_statement_with_multiline_expression() {
    let source = r#"
        func test() {
            val result = x +
                         y *
                         z
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}
