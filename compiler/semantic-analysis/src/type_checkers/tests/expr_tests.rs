use super::*;

#[test]
fn comparison_chain_less_less_rejected() {
    let mut checker = TypeChecker::new();

    let func = make_function(
        "main",
        vec![],
        Some("bool".to_string()),
        vec![Stmt::Expr(Expr::Binary {
            left: Box::new(Expr::Binary {
                left: Box::new(Expr::Literal(Literal::Integer(1, None), Span::new(0, 1))),
                op: BinaryOp::Less,
                right: Box::new(Expr::Literal(Literal::Integer(2, None), Span::new(4, 5))),
                span: Span::new(0, 5),
            }),
            op: BinaryOp::Less,
            right: Box::new(Expr::Literal(Literal::Integer(3, None), Span::new(8, 9))),
            span: Span::new(0, 9),
        })],
    );

    checker.check_function(&func);
    assert!(checker.has_errors());

    let errors = checker.into_errors();
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::ComparisonChain { .. })),
        "Expected ComparisonChain error, got: {:?}",
        errors
    );
}

#[test]
fn comparison_chain_mixed_operators_rejected() {
    let mut checker = TypeChecker::new();

    // a <= b > c
    let func = make_function(
        "main",
        vec![],
        Some("bool".to_string()),
        vec![Stmt::Expr(Expr::Binary {
            left: Box::new(Expr::Binary {
                left: Box::new(Expr::Literal(Literal::Integer(1, None), Span::new(0, 1))),
                op: BinaryOp::LessEqual,
                right: Box::new(Expr::Literal(Literal::Integer(2, None), Span::new(5, 6))),
                span: Span::new(0, 6),
            }),
            op: BinaryOp::Greater,
            right: Box::new(Expr::Literal(Literal::Integer(3, None), Span::new(9, 10))),
            span: Span::new(0, 10),
        })],
    );

    checker.check_function(&func);
    assert!(checker.has_errors());

    let errors = checker.into_errors();
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::ComparisonChain { .. })),
        "Expected ComparisonChain error, got: {:?}",
        errors
    );
}

#[test]
fn comparison_chain_equality_rejected() {
    let mut checker = TypeChecker::new();

    // a == b == c
    let func = make_function(
        "main",
        vec![],
        Some("bool".to_string()),
        vec![Stmt::Expr(Expr::Binary {
            left: Box::new(Expr::Binary {
                left: Box::new(Expr::Literal(Literal::Integer(1, None), Span::new(0, 1))),
                op: BinaryOp::Equal,
                right: Box::new(Expr::Literal(Literal::Integer(2, None), Span::new(5, 6))),
                span: Span::new(0, 6),
            }),
            op: BinaryOp::Equal,
            right: Box::new(Expr::Literal(Literal::Integer(3, None), Span::new(10, 11))),
            span: Span::new(0, 11),
        })],
    );

    checker.check_function(&func);
    assert!(checker.has_errors());

    let errors = checker.into_errors();
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::ComparisonChain { .. })),
        "Expected ComparisonChain error, got: {:?}",
        errors
    );
}

#[test]
fn single_comparison_accepted() {
    let mut checker = TypeChecker::new();

    // a < b — valid
    let func = make_function(
        "main",
        vec![],
        Some("bool".to_string()),
        vec![Stmt::Expr(Expr::Binary {
            left: Box::new(Expr::Literal(Literal::Integer(1, None), Span::new(0, 1))),
            op: BinaryOp::Less,
            right: Box::new(Expr::Literal(Literal::Integer(2, None), Span::new(4, 5))),
            span: Span::new(0, 5),
        })],
    );

    checker.check_function(&func);
    assert!(
        !checker.has_errors(),
        "Single comparison should be accepted, got errors: {:?}",
        checker.into_errors()
    );
}

#[test]
fn comparison_with_logical_and_accepted() {
    let mut checker = TypeChecker::new();

    // a < b && b < c — valid
    let func = make_function(
        "main",
        vec![],
        Some("bool".to_string()),
        vec![Stmt::Expr(Expr::Binary {
            left: Box::new(Expr::Binary {
                left: Box::new(Expr::Literal(Literal::Integer(1, None), Span::new(0, 1))),
                op: BinaryOp::Less,
                right: Box::new(Expr::Literal(Literal::Integer(2, None), Span::new(4, 5))),
                span: Span::new(0, 5),
            }),
            op: BinaryOp::And,
            right: Box::new(Expr::Binary {
                left: Box::new(Expr::Literal(Literal::Integer(2, None), Span::new(9, 10))),
                op: BinaryOp::Less,
                right: Box::new(Expr::Literal(Literal::Integer(3, None), Span::new(13, 14))),
                span: Span::new(9, 14),
            }),
            span: Span::new(0, 14),
        })],
    );

    checker.check_function(&func);
    assert!(
        !checker.has_errors(),
        "Comparison with && should be accepted, got errors: {:?}",
        checker.into_errors()
    );
}

#[test]
fn string_len_resolves_to_u64() {
    let mut checker = TypeChecker::new();

    // "hello".len()
    let expr = Expr::Call {
        func: Box::new(Expr::FieldAccess {
            object: Box::new(Expr::Literal(
                Literal::String("hello".to_string()),
                Span::new(0, 7),
            )),
            field: make_ident("len"),
            span: Span::new(0, 11),
        }),
        args: vec![],
        span: Span::new(0, 13),
    };

    let ty = checker.check_expr(&expr, None);
    assert_eq!(ty, Some(Type::U64));
    assert!(
        !checker.has_errors(),
        "string.len() should type-check cleanly, got: {:?}",
        checker.into_errors()
    );
}

#[test]
fn string_len_with_argument_rejected() {
    let mut checker = TypeChecker::new();

    // "hello".len(1) — len takes no arguments
    let expr = Expr::Call {
        func: Box::new(Expr::FieldAccess {
            object: Box::new(Expr::Literal(
                Literal::String("hello".to_string()),
                Span::new(0, 7),
            )),
            field: make_ident("len"),
            span: Span::new(0, 11),
        }),
        args: vec![Expr::Literal(Literal::Integer(1, None), Span::new(12, 13))],
        span: Span::new(0, 14),
    };

    checker.check_expr(&expr, None);
    assert!(checker.has_errors());
    let errors = checker.into_errors();
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::ArgumentCountMismatch { .. })),
        "Expected ArgumentCountMismatch, got: {:?}",
        errors
    );
}

#[test]
fn unknown_builtin_method_reports_method_not_found() {
    let mut checker = TypeChecker::new();

    // "hello".foo() — no such intrinsic on string
    let expr = Expr::Call {
        func: Box::new(Expr::FieldAccess {
            object: Box::new(Expr::Literal(
                Literal::String("hello".to_string()),
                Span::new(0, 7),
            )),
            field: make_ident("foo"),
            span: Span::new(0, 11),
        }),
        args: vec![],
        span: Span::new(0, 13),
    };

    checker.check_expr(&expr, None);
    assert!(checker.has_errors());
    let errors = checker.into_errors();
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::MethodNotFound { .. })),
        "Expected MethodNotFound, got: {:?}",
        errors
    );
}
