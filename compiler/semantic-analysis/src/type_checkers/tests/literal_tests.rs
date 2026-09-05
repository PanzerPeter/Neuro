use super::super::TypeChecker;
#[allow(unused_imports)]
use super::{make_function, make_ident, make_type, semantic_errors};
use crate::errors::TypeError;
use crate::types::Type;
use ast_types::{BinaryOp, Expr, Stmt, UnaryOp};
use shared_types::{Literal, Span};

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

    checker.register_function_signature(&func);

    checker.check_function(&func);
    assert!(!checker.has_errors());

    // Call with literal
    let call_expr = Expr::Call {
        func: Box::new(Expr::Identifier(make_ident("foo"))),
        arg_labels: Vec::new(),
        type_args: Vec::new(),
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
fn test_negated_literal_for_unsigned_target_is_rejected() {
    // `val x: u8 = -1` as the parser actually builds it: a negation over the
    // magnitude `1`, not a negative literal. Range-checking the DENOTED value is
    // what rejects it; checking the magnitude alone sees `1` fitting `u8`.
    let mut checker = TypeChecker::new();

    let stmt = Stmt::VarDecl {
        name: make_ident("x"),
        ty: Some(make_type("u8")),
        init: Some(Expr::Unary {
            op: UnaryOp::Negate,
            operand: Box::new(Expr::Literal(Literal::Integer(1, None), Span::new(12, 13))),
            span: Span::new(11, 13),
        }),
        mutable: false,
        span: Span::new(0, 13),
    };

    checker.check_stmt(&stmt);
    assert!(checker.has_errors());

    let errors = checker.into_errors();
    assert_eq!(errors.len(), 1);
    match &errors[0] {
        TypeError::NegativeLiteralForUnsignedType { magnitude, ty, .. } => {
            assert_eq!(*magnitude, 1);
            assert_eq!(*ty, Type::U8);
        }
        other => panic!("Expected NegativeLiteralForUnsignedType error, got {other}"),
    }
}

#[test]
fn test_negated_zero_for_unsigned_target_is_accepted() {
    // `-0` denotes 0, which every unsigned type holds. The rejection above is about
    // the value the expression denotes, not about the `-` token being present.
    let mut checker = TypeChecker::new();

    let stmt = Stmt::VarDecl {
        name: make_ident("x"),
        ty: Some(make_type("u8")),
        init: Some(Expr::Unary {
            op: UnaryOp::Negate,
            operand: Box::new(Expr::Literal(Literal::Integer(0, None), Span::new(12, 13))),
            span: Span::new(11, 13),
        }),
        mutable: false,
        span: Span::new(0, 13),
    };

    assert!(checker.check_stmt(&stmt).is_some());
    assert!(!checker.has_errors());
}

#[test]
fn test_negated_literal_out_of_range_for_signed_target_is_not_the_unsigned_error() {
    // `val x: i8 = -200` is out of range, but `i8` HAS negative values — it must not
    // pick up the unsigned diagnostic, which is keyed on the target's signedness.
    let mut checker = TypeChecker::new();

    let stmt = Stmt::VarDecl {
        name: make_ident("x"),
        ty: Some(make_type("i8")),
        init: Some(Expr::Unary {
            op: UnaryOp::Negate,
            operand: Box::new(Expr::Literal(
                Literal::Integer(200, None),
                Span::new(12, 15),
            )),
            span: Span::new(11, 15),
        }),
        mutable: false,
        span: Span::new(0, 15),
    };

    checker.check_stmt(&stmt);
    let errors = checker.into_errors();
    assert_eq!(errors.len(), 1);
    match &errors[0] {
        TypeError::IntegerLiteralOutOfRange { value, ty, .. } => {
            assert_eq!(*value, -200);
            assert_eq!(*ty, Type::I8);
        }
        other => panic!("Expected IntegerLiteralOutOfRange error, got {other}"),
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

    checker.register_function_signature(&func);

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
