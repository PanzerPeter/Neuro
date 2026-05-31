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

// Build `<recv>.<method>(<arg>)` where recv and arg are u8-suffixed literals.
fn int_intrinsic_call(method: &str, recv: i64, arg: i64) -> Expr {
    Expr::Call {
        func: Box::new(Expr::FieldAccess {
            object: Box::new(Expr::Literal(
                Literal::Integer(recv, Some(shared_types::IntSuffix::U8)),
                Span::new(0, 5),
            )),
            field: make_ident(method),
            span: Span::new(0, 20),
        }),
        args: vec![Expr::Literal(
            Literal::Integer(arg, Some(shared_types::IntSuffix::U8)),
            Span::new(21, 26),
        )],
        span: Span::new(0, 27),
    }
}

#[test]
fn integer_intrinsics_resolve_to_receiver_type() {
    for method in [
        "wrapping_add",
        "wrapping_sub",
        "wrapping_mul",
        "saturating_add",
        "saturating_sub",
        "saturating_mul",
        "shr",
    ] {
        let mut checker = TypeChecker::new();
        let ty = checker.check_expr(&int_intrinsic_call(method, 200, 100), None);
        assert_eq!(ty, Some(Type::U8), "method {method} should return U8");
        assert!(
            !checker.has_errors(),
            "{method} should type-check cleanly, got: {:?}",
            checker.into_errors()
        );
    }
}

#[test]
fn integer_intrinsic_wrong_arity_rejected() {
    let mut checker = TypeChecker::new();

    // 200u8.wrapping_add() — missing the rhs argument.
    let expr = Expr::Call {
        func: Box::new(Expr::FieldAccess {
            object: Box::new(Expr::Literal(
                Literal::Integer(200, Some(shared_types::IntSuffix::U8)),
                Span::new(0, 5),
            )),
            field: make_ident("wrapping_add"),
            span: Span::new(0, 20),
        }),
        args: vec![],
        span: Span::new(0, 22),
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
fn integer_intrinsic_mismatched_arg_type_rejected() {
    let mut checker = TypeChecker::new();

    // 200u8.wrapping_add(5i64) — argument type differs from the receiver's.
    let expr = Expr::Call {
        func: Box::new(Expr::FieldAccess {
            object: Box::new(Expr::Literal(
                Literal::Integer(200, Some(shared_types::IntSuffix::U8)),
                Span::new(0, 5),
            )),
            field: make_ident("wrapping_add"),
            span: Span::new(0, 20),
        }),
        args: vec![Expr::Literal(
            Literal::Integer(5, Some(shared_types::IntSuffix::I64)),
            Span::new(21, 25),
        )],
        span: Span::new(0, 27),
    };

    checker.check_expr(&expr, None);
    assert!(checker.has_errors());
    let errors = checker.into_errors();
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::Mismatch { .. })),
        "Expected Mismatch, got: {:?}",
        errors
    );
}

#[test]
fn integer_intrinsic_on_float_reports_method_not_found() {
    let mut checker = TypeChecker::new();

    // (1.5f64).wrapping_add(2.0) — no integer intrinsics on floats.
    let expr = Expr::Call {
        func: Box::new(Expr::FieldAccess {
            object: Box::new(Expr::Literal(Literal::Float(1.5, None), Span::new(0, 5))),
            field: make_ident("wrapping_add"),
            span: Span::new(0, 20),
        }),
        args: vec![Expr::Literal(Literal::Float(2.0, None), Span::new(21, 24))],
        span: Span::new(0, 26),
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
