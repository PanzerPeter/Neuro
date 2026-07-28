use super::super::TypeChecker;
#[allow(unused_imports)]
use super::{make_function, make_ident, make_type, semantic_errors};
use crate::errors::TypeError;
use crate::types::Type;
use ast_types::{Expr, Stmt};
use shared_types::{Literal, Span};

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

    let stmt = Stmt::Expr(Expr::Loop {
        label: Some(make_ident("outer")),
        body: vec![Stmt::Expr(Expr::Loop {
            label: None,
            body: vec![Stmt::Break {
                label: Some(make_ident("outer")),
                value: None,
                span: Span::new(0, 1),
            }],
            span: Span::new(0, 1),
        })],
        span: Span::new(0, 1),
    });

    checker.check_stmt(&stmt);
    assert!(!checker.has_errors());
}

#[test]
fn test_undefined_loop_label_is_rejected() {
    let mut checker = TypeChecker::new();

    let stmt = Stmt::Expr(Expr::Loop {
        label: Some(make_ident("outer")),
        body: vec![Stmt::Break {
            label: Some(make_ident("missing")),
            value: None,
            span: Span::new(0, 1),
        }],
        span: Span::new(0, 1),
    });

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
