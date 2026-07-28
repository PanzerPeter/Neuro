use super::super::*;
use super::*;

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
        type_args: Vec::new(),
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
        type_args: Vec::new(),
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
fn string_clone_resolves_to_string() {
    let mut checker = TypeChecker::new();

    // "hello".clone()
    let expr = Expr::Call {
        func: Box::new(Expr::FieldAccess {
            object: Box::new(Expr::Literal(
                Literal::String("hello".to_string()),
                Span::new(0, 7),
            )),
            field: make_ident("clone"),
            span: Span::new(0, 13),
        }),
        type_args: Vec::new(),
        args: vec![],
        span: Span::new(0, 15),
    };

    let ty = checker.check_expr(&expr, None);
    assert_eq!(ty, Some(Type::String));
    assert!(
        !checker.has_errors(),
        "string.clone() should type-check cleanly, got: {:?}",
        checker.into_errors()
    );
}

#[test]
fn string_clone_with_argument_rejected() {
    let mut checker = TypeChecker::new();

    // "hello".clone(1) — clone takes no arguments
    let expr = Expr::Call {
        func: Box::new(Expr::FieldAccess {
            object: Box::new(Expr::Literal(
                Literal::String("hello".to_string()),
                Span::new(0, 7),
            )),
            field: make_ident("clone"),
            span: Span::new(0, 13),
        }),
        type_args: Vec::new(),
        args: vec![Expr::Literal(Literal::Integer(1, None), Span::new(14, 15))],
        span: Span::new(0, 16),
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
fn string_slice_resolves_to_string_reference() {
    let mut checker = TypeChecker::new();

    // "hello".slice(0..3)
    let expr = Expr::Call {
        func: Box::new(Expr::FieldAccess {
            object: Box::new(Expr::Literal(
                Literal::String("hello".to_string()),
                Span::new(0, 7),
            )),
            field: make_ident("slice"),
            span: Span::new(0, 13),
        }),
        type_args: Vec::new(),
        args: vec![Expr::Range {
            start: Box::new(Expr::Literal(Literal::Integer(0, None), Span::new(14, 15))),
            end: Box::new(Expr::Literal(Literal::Integer(3, None), Span::new(17, 18))),
            inclusive: false,
            span: Span::new(14, 18),
        }],
        span: Span::new(0, 19),
    };

    let ty = checker.check_expr(&expr, None);
    assert_eq!(
        ty,
        Some(Type::Reference {
            inner: Box::new(Type::String),
            mutable: false,
        })
    );
    assert!(
        !checker.has_errors(),
        "string.slice(range) should type-check cleanly, got: {:?}",
        checker.into_errors()
    );
}

#[test]
fn string_slice_without_range_is_rejected() {
    let mut checker = TypeChecker::new();

    // "hello".slice(3) — argument must be a range, not a bare integer
    let expr = Expr::Call {
        func: Box::new(Expr::FieldAccess {
            object: Box::new(Expr::Literal(
                Literal::String("hello".to_string()),
                Span::new(0, 7),
            )),
            field: make_ident("slice"),
            span: Span::new(0, 13),
        }),
        type_args: Vec::new(),
        args: vec![Expr::Literal(Literal::Integer(3, None), Span::new(14, 15))],
        span: Span::new(0, 16),
    };

    checker.check_expr(&expr, None);
    let errors = checker.into_errors();
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::SliceExpectsRange { .. })),
        "Expected SliceExpectsRange, got: {:?}",
        errors
    );
}

#[test]
fn range_outside_slice_is_rejected() {
    let mut checker = TypeChecker::new();

    // 0..5 used as a standalone value
    let expr = Expr::Range {
        start: Box::new(Expr::Literal(Literal::Integer(0, None), Span::new(0, 1))),
        end: Box::new(Expr::Literal(Literal::Integer(5, None), Span::new(3, 4))),
        inclusive: false,
        span: Span::new(0, 4),
    };

    checker.check_expr(&expr, None);
    let errors = checker.into_errors();
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::RangeNotAllowed { .. })),
        "Expected RangeNotAllowed, got: {:?}",
        errors
    );
}

/// `string + string` type-checks to `string` (concatenation), distinct from
/// the numeric arithmetic path.
#[test]
fn string_concat_yields_string() {
    let mut checker = TypeChecker::new();

    let concat = Expr::Binary {
        left: Box::new(Expr::Literal(
            Literal::String("foo".to_string()),
            Span::new(0, 5),
        )),
        op: BinaryOp::Add,
        right: Box::new(Expr::Literal(
            Literal::String("bar".to_string()),
            Span::new(8, 13),
        )),
        span: Span::new(0, 13),
    };

    let ty = checker.check_expr(&concat, None);
    assert_eq!(ty, Some(Type::String));
    assert!(!checker.has_errors(), "got: {:?}", checker.into_errors());
}

/// Only `+` joins strings; `string - string` is an invalid-operator error.
#[test]
fn string_subtract_is_rejected() {
    let mut checker = TypeChecker::new();

    let sub = Expr::Binary {
        left: Box::new(Expr::Literal(
            Literal::String("foo".to_string()),
            Span::new(0, 5),
        )),
        op: BinaryOp::Subtract,
        right: Box::new(Expr::Literal(
            Literal::String("bar".to_string()),
            Span::new(8, 13),
        )),
        span: Span::new(0, 13),
    };

    let ty = checker.check_expr(&sub, None);
    assert_eq!(ty, Some(Type::Unknown));
    assert!(checker
        .into_errors()
        .iter()
        .any(|e| matches!(e, TypeError::InvalidBinaryOperator { .. })));
}

/// Mixing a string with a non-string under `+` is rejected (no silent coercion).
#[test]
fn string_plus_integer_is_rejected() {
    let mut checker = TypeChecker::new();

    let mixed = Expr::Binary {
        left: Box::new(Expr::Literal(
            Literal::String("foo".to_string()),
            Span::new(0, 5),
        )),
        op: BinaryOp::Add,
        right: Box::new(Expr::Literal(Literal::Integer(1, None), Span::new(8, 9))),
        span: Span::new(0, 9),
    };

    let ty = checker.check_expr(&mixed, None);
    assert_eq!(ty, Some(Type::Unknown));
    assert!(checker
        .into_errors()
        .iter()
        .any(|e| matches!(e, TypeError::InvalidBinaryOperator { .. })));
}
