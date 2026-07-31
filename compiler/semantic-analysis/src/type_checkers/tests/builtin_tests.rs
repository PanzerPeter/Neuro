use super::super::*;
use super::*;

#[test]
fn unsafe_block_yields_trailing_expr_type_without_errors() {
    // func main() -> i32 { unsafe { 7 } }
    let mut checker = TypeChecker::new();

    let func = make_function(
        "main",
        vec![],
        Some("i32".to_string()),
        vec![Stmt::Expr(Expr::Unsafe {
            stmts: vec![Stmt::Expr(Expr::Literal(
                Literal::Integer(7, None),
                Span::new(9, 10),
            ))],
            span: Span::new(0, 12),
        })],
    );

    checker.register_function_signature(&func);

    checker.check_function(&func);
    assert!(
        !checker.has_errors(),
        "unsafe block should type-check cleanly, got: {:?}",
        checker.into_errors()
    );
}

/// `panic("boom")` is a builtin returning unit; a correct call type-checks cleanly.
#[test]
fn panic_builtin_accepts_string_argument() {
    let mut checker = TypeChecker::new();

    let call = Expr::Call {
        func: Box::new(Expr::Identifier(make_ident("panic"))),
        type_args: Vec::new(),
        args: vec![Expr::Literal(
            Literal::String("boom".to_string()),
            Span::new(6, 12),
        )],
        span: Span::new(0, 13),
    };

    let ty = checker.check_expr(&call, None);
    assert_eq!(ty, Some(Type::Unknown));
    assert!(!checker.has_errors(), "got: {:?}", checker.into_errors());
}

/// `assert(cond)` requires a bool; a non-bool argument is a type error.
#[test]
fn assert_builtin_rejects_non_bool_argument() {
    let mut checker = TypeChecker::new();

    let call = Expr::Call {
        func: Box::new(Expr::Identifier(make_ident("assert"))),
        type_args: Vec::new(),
        args: vec![Expr::Literal(Literal::Integer(1, None), Span::new(7, 8))],
        span: Span::new(0, 9),
    };

    let ty = checker.check_expr(&call, None);
    assert_eq!(ty, Some(Type::Unknown));
    assert!(checker
        .into_errors()
        .iter()
        .any(|e| matches!(e, TypeError::Mismatch { .. })));
}

/// `unreachable()` is nullary; passing an argument is an arity error.
#[test]
fn unreachable_builtin_rejects_arguments() {
    let mut checker = TypeChecker::new();

    let call = Expr::Call {
        func: Box::new(Expr::Identifier(make_ident("unreachable"))),
        type_args: Vec::new(),
        args: vec![Expr::Literal(Literal::Integer(1, None), Span::new(12, 13))],
        span: Span::new(0, 14),
    };

    let ty = checker.check_expr(&call, None);
    assert_eq!(ty, Some(Type::Unknown));
    assert!(checker
        .into_errors()
        .iter()
        .any(|e| matches!(e, TypeError::ArgumentCountMismatch { .. })));
}

/// A user-defined `func panic(n: i32) -> i32` shadows the builtin: the call is
/// checked against the user signature (returns i32, accepts an integer).
#[test]
fn user_function_shadows_panic_builtin() {
    let mut checker = TypeChecker::new();

    let func = make_function(
        "panic",
        vec![("n".to_string(), "i32".to_string())],
        Some("i32".to_string()),
        vec![Stmt::Return {
            value: Some(Expr::Identifier(make_ident("n"))),
            span: Span::new(0, 8),
        }],
    );
    checker.register_function_signature(&func);
    checker.check_function(&func);
    assert!(!checker.has_errors());

    let call = Expr::Call {
        func: Box::new(Expr::Identifier(make_ident("panic"))),
        type_args: Vec::new(),
        args: vec![Expr::Literal(Literal::Integer(7, None), Span::new(6, 7))],
        span: Span::new(0, 8),
    };

    let ty = checker.check_expr(&call, None);
    assert_eq!(ty, Some(Type::I32));
    assert!(!checker.has_errors(), "got: {:?}", checker.into_errors());
}
