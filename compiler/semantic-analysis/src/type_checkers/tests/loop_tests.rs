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
        index: None,
        iterator: make_ident("i"),
        start: Expr::Literal(Literal::Integer(0, None), Span::new(0, 1)),
        end: Expr::Literal(Literal::Integer(5, None), Span::new(4, 5)),
        inclusive: false,
        adapters: Vec::new(),
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
        index: None,
        iterator: make_ident("i"),
        start: Expr::Literal(Literal::Boolean(true), Span::new(0, 4)),
        end: Expr::Literal(Literal::Integer(5, None), Span::new(7, 8)),
        inclusive: false,
        adapters: Vec::new(),
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

/// The position binding is `u64` whatever the sequence holds, so it indexes the
/// sequence it walks without a cast.
#[test]
fn enumerated_index_is_u64_and_indexes_its_sequence() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val a: [i32; 3] = [1, 2, 3]
    mut total: i32 = 0
    for (i, x) in a.enumerate() {
        val n: u64 = i
        total = total + a[i] + x
    }
    for (k, v) in (0..3).enumerate() {
        val m: u64 = k
        total = total + v
    }
    return 0
}
"#,
    );
    assert!(errors.is_empty(), "valid enumerated loops; got {errors:?}");
}

#[test]
fn enumerated_index_does_not_escape_the_loop() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val a: [i32; 3] = [1, 2, 3]
    for (i, x) in a.enumerate() {
        return x
    }
    return i as i32
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, TypeError::UndefinedVariable { .. })),
        "index should be scoped to the loop; got {errors:?}"
    );
}

/// The two bindings share one scope, so a head that names them alike is a
/// redefinition rather than a silent shadow.
#[test]
fn enumerated_head_rejects_a_repeated_name() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val a: [i32; 3] = [1, 2, 3]
    for (i, i) in a.enumerate() {
        return 0
    }
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, TypeError::VariableAlreadyDefined { .. })),
        "expected a redefinition error; got {errors:?}"
    );
}
