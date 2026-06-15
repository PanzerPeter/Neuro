use super::TypeChecker;
use crate::errors::TypeError;
use crate::types::Type;
use ast_types::{BinaryOp, Expr, FunctionDef, Parameter, Stmt};
use shared_types::{Identifier, Literal, Span};

fn make_ident(name: &str) -> Identifier {
    Identifier {
        name: name.to_string(),
        span: Span::new(0, 0),
    }
}

fn make_type(name: &str) -> ast_types::Type {
    ast_types::Type::Named(make_ident(name))
}

/// Helper to create a simple function for testing
fn make_function(
    name: &str,
    params: Vec<(String, String)>,
    return_type: Option<String>,
    body: Vec<Stmt>,
) -> FunctionDef {
    FunctionDef {
        name: make_ident(name),
        params: params
            .into_iter()
            .map(|(pname, pty)| Parameter {
                name: make_ident(&pname),
                ty: make_type(&pty),
                span: Span::new(0, 0),
            })
            .collect(),
        return_type: return_type.map(|rt| make_type(&rt)),
        body,
        attributes: Vec::new(),
        span: Span::new(0, 0),
    }
}

#[test]
fn test_integer_literal_infers_from_variable_declaration() {
    // val x: i64 = 42
    // The literal 42 should infer as i64, not i32
    let mut checker = TypeChecker::new();

    let stmt = Stmt::VarDecl {
        name: make_ident("x"),
        ty: Some(make_type("i64")),
        init: Some(Expr::Literal(Literal::Integer(42, None), Span::new(0, 2))),
        mutable: false,
        span: Span::new(0, 10),
    };

    assert!(checker.check_stmt(&stmt).is_some());
    assert!(!checker.has_errors());

    // Verify variable has correct type
    let symbol_info = checker.symbols.lookup("x").unwrap();
    assert_eq!(symbol_info.ty, Type::I64);
}

#[test]
fn test_integer_literal_infers_from_function_parameter() {
    // func foo(x: u32) {}
    // foo(42) - the literal 42 should infer as u32
    let mut checker = TypeChecker::new();

    // Define function
    let func = make_function(
        "foo",
        vec![("x".to_string(), "u32".to_string())],
        None,
        vec![],
    );

    checker.check_function(&func);
    assert!(!checker.has_errors());

    // Call with literal
    let call_expr = Expr::Call {
        func: Box::new(Expr::Identifier(make_ident("foo"))),
        args: vec![Expr::Literal(Literal::Integer(42, None), Span::new(0, 2))],
        span: Span::new(0, 10),
    };

    let result_ty = checker.check_expr(&call_expr, None);
    assert_eq!(result_ty, Some(Type::Void));
    assert!(!checker.has_errors());
}

#[test]
fn test_integer_literal_out_of_range_i8() {
    // val x: i8 = 300  - should error (i8 max is 127)
    let mut checker = TypeChecker::new();

    let stmt = Stmt::VarDecl {
        name: make_ident("x"),
        ty: Some(make_type("i8")),
        init: Some(Expr::Literal(Literal::Integer(300, None), Span::new(0, 3))),
        mutable: false,
        span: Span::new(0, 10),
    };

    checker.check_stmt(&stmt);
    assert!(checker.has_errors());

    let errors = checker.into_errors();
    assert_eq!(errors.len(), 1);
    match &errors[0] {
        TypeError::IntegerLiteralOutOfRange { value, ty, .. } => {
            assert_eq!(*value, 300);
            assert_eq!(*ty, Type::I8);
        }
        _ => panic!("Expected IntegerLiteralOutOfRange error"),
    }
}

#[test]
fn test_integer_literal_negative_u32() {
    // val x: u32 = -42  - should error (u32 can't be negative)
    let mut checker = TypeChecker::new();

    let stmt = Stmt::VarDecl {
        name: make_ident("x"),
        ty: Some(make_type("u32")),
        init: Some(Expr::Literal(Literal::Integer(-42, None), Span::new(0, 3))),
        mutable: false,
        span: Span::new(0, 10),
    };

    checker.check_stmt(&stmt);
    assert!(checker.has_errors());

    let errors = checker.into_errors();
    assert_eq!(errors.len(), 1);
    match &errors[0] {
        TypeError::IntegerLiteralOutOfRange { value, ty, .. } => {
            assert_eq!(*value, -42);
            assert_eq!(*ty, Type::U32);
        }
        _ => panic!("Expected IntegerLiteralOutOfRange error"),
    }
}

#[test]
fn test_float_literal_infers_f32() {
    // val x: f32 = 2.5
    // The literal 2.5 should infer as f32, not f64
    let mut checker = TypeChecker::new();

    let stmt = Stmt::VarDecl {
        name: make_ident("x"),
        ty: Some(make_type("f32")),
        init: Some(Expr::Literal(Literal::Float(2.5, None), Span::new(0, 3))),
        mutable: false,
        span: Span::new(0, 10),
    };

    assert!(checker.check_stmt(&stmt).is_some());
    assert!(!checker.has_errors());

    // Verify variable has correct type
    let symbol_info = checker.symbols.lookup("x").unwrap();
    assert_eq!(symbol_info.ty, Type::F32);
}

#[test]
fn test_literal_inference_in_return() {
    // func foo() -> i16 { 42 }
    // The literal 42 should infer as i16
    let mut checker = TypeChecker::new();

    let func = make_function(
        "foo",
        vec![],
        Some("i16".to_string()),
        vec![Stmt::Expr(Expr::Literal(
            Literal::Integer(42, None),
            Span::new(0, 2),
        ))],
    );

    checker.check_function(&func);
    assert!(!checker.has_errors());
}

#[test]
fn test_literal_inference_in_assignment() {
    // mut x: u64 = 100
    // x = 200
    // The literal 200 should infer as u64
    let mut checker = TypeChecker::new();

    let decl = Stmt::VarDecl {
        name: make_ident("x"),
        ty: Some(make_type("u64")),
        init: Some(Expr::Literal(Literal::Integer(100, None), Span::new(0, 3))),
        mutable: true,
        span: Span::new(0, 15),
    };

    checker.check_stmt(&decl);
    assert!(!checker.has_errors());

    let assign = Stmt::Assignment {
        target: make_ident("x"),
        value: Expr::Literal(Literal::Integer(200, None), Span::new(0, 3)),
        span: Span::new(0, 7),
    };

    checker.check_stmt(&assign);
    assert!(!checker.has_errors());
}

#[test]
fn test_literal_defaults_to_i32_without_context() {
    // val x = 42 (no type annotation)
    // Should default to i32
    let mut checker = TypeChecker::new();

    let stmt = Stmt::VarDecl {
        name: make_ident("x"),
        ty: None,
        init: Some(Expr::Literal(Literal::Integer(42, None), Span::new(0, 2))),
        mutable: false,
        span: Span::new(0, 10),
    };

    checker.check_stmt(&stmt);
    assert!(!checker.has_errors());

    // Verify variable has i32 type
    let symbol_info = checker.symbols.lookup("x").unwrap();
    assert_eq!(symbol_info.ty, Type::I32);
}

#[test]
fn test_literal_defaults_to_f64_without_context() {
    // val x = 2.5 (no type annotation)
    // Should default to f64
    let mut checker = TypeChecker::new();

    let stmt = Stmt::VarDecl {
        name: make_ident("x"),
        ty: None,
        init: Some(Expr::Literal(Literal::Float(2.5, None), Span::new(0, 3))),
        mutable: false,
        span: Span::new(0, 10),
    };

    checker.check_stmt(&stmt);
    assert!(!checker.has_errors());

    // Verify variable has f64 type
    let symbol_info = checker.symbols.lookup("x").unwrap();
    assert_eq!(symbol_info.ty, Type::F64);
}

#[test]
fn test_literal_inference_in_binary_operation() {
    // val x: i16 = 10
    // val y: i16 = x + 5
    // The literal 5 should infer as i16 from x
    let mut checker = TypeChecker::new();

    let decl_x = Stmt::VarDecl {
        name: make_ident("x"),
        ty: Some(make_type("i16")),
        init: Some(Expr::Literal(Literal::Integer(10, None), Span::new(0, 2))),
        mutable: false,
        span: Span::new(0, 10),
    };

    checker.check_stmt(&decl_x);
    assert!(!checker.has_errors());

    let decl_y = Stmt::VarDecl {
        name: make_ident("y"),
        ty: Some(make_type("i16")),
        init: Some(Expr::Binary {
            left: Box::new(Expr::Identifier(make_ident("x"))),
            op: BinaryOp::Add,
            right: Box::new(Expr::Literal(Literal::Integer(5, None), Span::new(0, 1))),
            span: Span::new(0, 5),
        }),
        mutable: false,
        span: Span::new(0, 15),
    };

    checker.check_stmt(&decl_y);
    assert!(!checker.has_errors());
}

#[test]
fn test_large_literal_fails_to_promote_to_i64() {
    // val x = 5000000000  (too large for i32)
    // Should NOT automatically use i64
    let mut checker = TypeChecker::new();

    let stmt = Stmt::VarDecl {
        name: make_ident("x"),
        ty: None,
        init: Some(Expr::Literal(
            Literal::Integer(5000000000, None),
            Span::new(0, 10),
        )),
        mutable: false,
        span: Span::new(0, 15),
    };

    checker.check_stmt(&stmt);
    assert!(checker.has_errors());

    let errors = checker.into_errors();
    assert_eq!(errors.len(), 1);
    match &errors[0] {
        TypeError::IntegerLiteralOutOfRange { value, ty, .. } => {
            assert_eq!(*value, 5000000000);
            assert_eq!(*ty, Type::I32);
        }
        _ => panic!("Expected IntegerLiteralOutOfRange error"),
    }
}

#[test]
fn test_for_range_accepts_integer_bounds() {
    let mut checker = TypeChecker::new();

    let stmt = Stmt::ForRange {
        label: None,
        iterator: make_ident("i"),
        start: Expr::Literal(Literal::Integer(0, None), Span::new(0, 1)),
        end: Expr::Literal(Literal::Integer(5, None), Span::new(4, 5)),
        inclusive: false,
        body: vec![Stmt::Continue {
            label: None,
            span: Span::new(8, 16),
        }],
        span: Span::new(0, 16),
    };

    checker.check_stmt(&stmt);
    assert!(!checker.has_errors());
}

#[test]
fn test_for_range_rejects_non_integer_bound() {
    let mut checker = TypeChecker::new();

    let stmt = Stmt::ForRange {
        label: None,
        iterator: make_ident("i"),
        start: Expr::Literal(Literal::Boolean(true), Span::new(0, 4)),
        end: Expr::Literal(Literal::Integer(5, None), Span::new(7, 8)),
        inclusive: false,
        body: vec![],
        span: Span::new(0, 12),
    };

    checker.check_stmt(&stmt);
    assert!(checker.has_errors());

    let errors = checker.into_errors();
    assert!(errors
        .iter()
        .any(|error| matches!(error, TypeError::InvalidForRangeType { .. })));
}

#[test]
fn test_labeled_break_resolves_to_enclosing_loop() {
    let mut checker = TypeChecker::new();

    let stmt = Stmt::Loop {
        label: Some(make_ident("outer")),
        body: vec![Stmt::Loop {
            label: None,
            body: vec![Stmt::Break {
                label: Some(make_ident("outer")),
                value: None,
                span: Span::new(0, 1),
            }],
            span: Span::new(0, 1),
        }],
        span: Span::new(0, 1),
    };

    checker.check_stmt(&stmt);
    assert!(!checker.has_errors());
}

#[test]
fn test_undefined_loop_label_is_rejected() {
    let mut checker = TypeChecker::new();

    let stmt = Stmt::Loop {
        label: Some(make_ident("outer")),
        body: vec![Stmt::Break {
            label: Some(make_ident("missing")),
            value: None,
            span: Span::new(0, 1),
        }],
        span: Span::new(0, 1),
    };

    checker.check_stmt(&stmt);
    assert!(checker.has_errors());
    let errors = checker.into_errors();
    assert!(errors
        .iter()
        .any(|error| matches!(error, TypeError::UndefinedLabel { .. })));
}

#[test]
fn test_break_outside_loop_still_rejected() {
    let mut checker = TypeChecker::new();

    let stmt = Stmt::Break {
        label: None,
        value: None,
        span: Span::new(0, 1),
    };

    checker.check_stmt(&stmt);
    let errors = checker.into_errors();
    assert!(errors
        .iter()
        .any(|error| matches!(error, TypeError::BreakOutsideLoop { .. })));
}

#[test]
fn test_loop_expression_takes_break_value_type() {
    let mut checker = TypeChecker::new();

    // loop { break 42 }
    let loop_expr = Expr::Loop {
        label: None,
        body: vec![Stmt::Break {
            label: None,
            value: Some(Expr::Literal(Literal::Integer(42, None), Span::new(0, 1))),
            span: Span::new(0, 1),
        }],
        span: Span::new(0, 1),
    };

    let ty = checker.check_expr(&loop_expr, None);
    assert!(!checker.has_errors());
    assert_eq!(ty, Some(Type::I32));
}

#[test]
fn test_break_value_type_disagreement_is_rejected() {
    let mut checker = TypeChecker::new();

    // loop { break 1 \n break "two" }
    let loop_expr = Expr::Loop {
        label: None,
        body: vec![
            Stmt::Break {
                label: None,
                value: Some(Expr::Literal(Literal::Integer(1, None), Span::new(0, 1))),
                span: Span::new(0, 1),
            },
            Stmt::Break {
                label: None,
                value: Some(Expr::Literal(
                    Literal::String("two".to_string()),
                    Span::new(2, 3),
                )),
                span: Span::new(2, 3),
            },
        ],
        span: Span::new(0, 3),
    };

    let _ = checker.check_expr(&loop_expr, None);
    let errors = checker.into_errors();
    assert!(errors
        .iter()
        .any(|error| matches!(error, TypeError::Mismatch { .. })));
}

#[test]
fn test_break_value_in_while_loop_is_rejected() {
    let mut checker = TypeChecker::new();

    // while true { break 5 } — `while` always yields unit.
    let stmt = Stmt::While {
        label: None,
        condition: Expr::Literal(Literal::Boolean(true), Span::new(0, 1)),
        body: vec![Stmt::Break {
            label: None,
            value: Some(Expr::Literal(Literal::Integer(5, None), Span::new(2, 3))),
            span: Span::new(2, 3),
        }],
        span: Span::new(0, 3),
    };

    checker.check_stmt(&stmt);
    let errors = checker.into_errors();
    assert!(errors
        .iter()
        .any(|error| matches!(error, TypeError::BreakValueInUnitLoop { .. })));
}
