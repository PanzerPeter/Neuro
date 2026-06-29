// Statement parsing tests

use syntax_parsing::{parse, Item, Stmt};

/// Count the statements the first function body desugars to.
fn first_fn_body_len(source: &str) -> usize {
    let items = parse(source).expect("parse failed");
    for item in &items {
        if let Item::Function(func) = item {
            return func.body.len();
        }
    }
    panic!("no function found");
}

#[test]
fn test_tuple_destructure_desugars_to_temp_plus_bindings() {
    // §3.2: `val (a, b) = e` expands to a temp binding plus one bind per leaf — so
    // three statements here. A `_` wildcard binds nothing.
    let three = r#"
        func test() {
            val (a, b) = pair
        }
    "#;
    // temp + a + b = 3
    assert_eq!(first_fn_body_len(three), 3);

    let wildcard = r#"
        func test() {
            val (_, keep, _) = triple
        }
    "#;
    // temp + keep = 2 (the two `_` leaves bind nothing)
    assert_eq!(first_fn_body_len(wildcard), 2);
}

#[test]
fn test_tuple_destructure_temp_is_a_var_decl() {
    let source = r#"
        func test() {
            val (a, b) = pair
        }
    "#;
    let items = parse(source).expect("parse failed");
    let Item::Function(func) = &items[0] else {
        panic!("expected function");
    };
    assert!(
        matches!(func.body.first(), Some(Stmt::VarDecl { .. })),
        "first desugared statement should be the temp VarDecl"
    );
}

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
fn test_parse_while_statement() {
    let source = r#"
        func test() {
            mut x: i32 = 0
            while x < 10 {
                x = x + 1
            }
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_parse_nested_while_in_if() {
    let source = r#"
        func test() {
            mut x: i32 = 0
            if true {
                while x < 3 {
                    x = x + 1
                }
            }
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_parse_loop_statement() {
    let source = r#"
        func test() {
            mut x: i32 = 0
            loop {
                x = x + 1
                if x > 5 {
                    break
                }
            }
        }
    "#;
    let items = parse(source).expect("loop statement should parse");
    let ast_types::Item::Function(func) = &items[0] else {
        panic!("expected a function item");
    };
    assert!(
        matches!(func.body.get(1), Some(ast_types::Stmt::Loop { .. })),
        "second statement should be a Stmt::Loop, got {:?}",
        func.body.get(1)
    );
}

#[test]
fn test_parse_for_range_statement() {
    let source = r#"
        func test() {
            for i in 0..10 {
                val x = i
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

#[test]
fn test_parse_labeled_for_with_labeled_break() {
    let source = r#"
        func test() {
            outer: for i in 0..10 {
                for j in 0..10 {
                    if i * j > 5 {
                        break outer
                    }
                }
            }
        }
    "#;
    let items = parse(source).expect("labeled for with labeled break should parse");
    let ast_types::Item::Function(func) = &items[0] else {
        panic!("expected a function item");
    };
    let Some(ast_types::Stmt::ForRange { label, body, .. }) = func.body.first() else {
        panic!("first statement should be a labeled for-range");
    };
    assert_eq!(label.as_ref().map(|l| l.name.as_str()), Some("outer"));

    let Some(ast_types::Stmt::ForRange { body: inner, .. }) = body.first() else {
        panic!("inner statement should be a for-range");
    };
    let Some(ast_types::Stmt::If { then_block, .. }) = inner.first() else {
        panic!("inner loop should contain an if");
    };
    assert!(
        matches!(
            then_block.first(),
            Some(ast_types::Stmt::Break { label: Some(l), .. }) if l.name == "outer"
        ),
        "expected `break outer`, got {:?}",
        then_block.first()
    );
}

#[test]
fn test_parse_labeled_loop_and_while() {
    let source = r#"
        func test() {
            search: loop {
                continue search
            }
            spin: while true {
                break spin
            }
        }
    "#;
    let items = parse(source).expect("labeled loop and while should parse");
    let ast_types::Item::Function(func) = &items[0] else {
        panic!("expected a function item");
    };
    assert!(matches!(
        func.body.first(),
        Some(ast_types::Stmt::Loop { label: Some(l), .. }) if l.name == "search"
    ));
    assert!(matches!(
        func.body.get(1),
        Some(ast_types::Stmt::While { label: Some(l), .. }) if l.name == "spin"
    ));
}

#[test]
fn test_unlabeled_break_has_no_label() {
    let source = r#"
        func test() {
            loop {
                break
            }
        }
    "#;
    let items = parse(source).expect("plain break should still parse");
    let ast_types::Item::Function(func) = &items[0] else {
        panic!("expected a function item");
    };
    let Some(ast_types::Stmt::Loop { body, .. }) = func.body.first() else {
        panic!("expected a loop");
    };
    assert!(matches!(
        body.first(),
        Some(ast_types::Stmt::Break {
            label: None,
            value: None,
            ..
        })
    ));
}

#[test]
fn test_parse_loop_value_expression() {
    let source = r#"
        func test() -> i32 {
            val x = loop {
                break 5
            }
            return x
        }
    "#;
    let items = parse(source).expect("loop value expression should parse");
    let ast_types::Item::Function(func) = &items[0] else {
        panic!("expected a function item");
    };
    let Some(ast_types::Stmt::VarDecl {
        init: Some(init), ..
    }) = func.body.first()
    else {
        panic!("first statement should be a var decl");
    };
    let ast_types::Expr::Loop { body, .. } = init else {
        panic!("initializer should be a loop expression, got {:?}", init);
    };
    // The `break 5` carries an integer value rather than being read as a label.
    assert!(matches!(
        body.first(),
        Some(ast_types::Stmt::Break {
            label: None,
            value: Some(_),
            ..
        })
    ));
}

#[test]
fn test_break_value_is_not_parsed_as_label() {
    // A bare identifier after `break` that is not an in-scope loop label is the
    // start of a value expression, not a label.
    let source = r#"
        func test() -> i32 {
            mut n: i32 = 7
            val x = loop {
                break n
            }
            return x
        }
    "#;
    let items = parse(source).expect("break with a non-label identifier value should parse");
    let ast_types::Item::Function(func) = &items[0] else {
        panic!("expected a function item");
    };
    let Some(ast_types::Stmt::VarDecl {
        init: Some(ast_types::Expr::Loop { body, .. }),
        ..
    }) = func.body.get(1)
    else {
        panic!("second statement should be a loop-value var decl");
    };
    assert!(matches!(
        body.first(),
        Some(ast_types::Stmt::Break {
            label: None,
            value: Some(ast_types::Expr::Identifier(_)),
            ..
        })
    ));
}
