use super::super::*;
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

    checker.register_function_signature(&func);

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

    checker.register_function_signature(&func);

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

    checker.register_function_signature(&func);

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

    checker.register_function_signature(&func);

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

    checker.register_function_signature(&func);

    checker.check_function(&func);
    assert!(
        !checker.has_errors(),
        "Comparison with && should be accepted, got errors: {:?}",
        checker.into_errors()
    );
}

/// `==` on a type with no built-in equality and no `PartialEq` impl is a type error.
/// Without the rejection the program reaches the backend, which asks a struct value for
/// its integer variant and aborts the compiler.
#[test]
fn equality_on_a_struct_without_partial_eq_is_rejected() {
    let errors = semantic_errors(
        r#"
        struct V { x: i32 }
        func main() -> i32 {
            val a = V { x: 1 }
            val b = V { x: 2 }
            if a == b { return 1 }
            return 0
        }
        "#,
    );
    assert!(
        errors.iter().any(
            |e| matches!(e, TypeError::MissingPartialEqImpl { type_name, .. } if type_name == "V")
        ),
        "a struct without PartialEq must be rejected, got {errors:?}"
    );
}

/// The supported path stays supported: a `Copy` struct with an explicit `impl PartialEq`
/// dispatches `==` to its `eq` method.
#[test]
fn equality_on_a_struct_with_partial_eq_is_accepted() {
    let errors = semantic_errors(
        r#"
        @derive(Copy, Clone)
        struct V { x: i32 }
        impl PartialEq for V {
            func eq(&self, rhs: &V) -> bool { self.x == rhs.x }
            func ne(&self, rhs: &V) -> bool { self.x != rhs.x }
        }
        func main() -> i32 {
            val a = V { x: 1 }
            val b = V { x: 2 }
            if a == b { return 1 }
            return 0
        }
        "#,
    );
    assert!(
        errors.is_empty(),
        "an explicit PartialEq impl must keep `==` working, got {errors:?}"
    );
}

/// Aggregates have no built-in equality either — the same missing rejection crashed the
/// backend for arrays, tuples, enums and non-string references.
#[test]
fn equality_on_an_aggregate_is_rejected() {
    for (name, program) in [
        (
            "array",
            "func main() -> i32 { val a = [1, 2]\nval b = [1, 2]\nif a == b { return 1 }\nreturn 0 }",
        ),
        (
            "tuple",
            "func main() -> i32 { val a = (1, 2)\nval b = (1, 2)\nif a == b { return 1 }\nreturn 0 }",
        ),
        (
            "enum",
            "enum E { A, B }\nfunc main() -> i32 { val a = E::A\nval b = E::B\nif a == b { return 1 }\nreturn 0 }",
        ),
        (
            "reference",
            "func main() -> i32 { val x = 1\nval y = 2\nval a = &x\nval b = &y\nif a == b { return 1 }\nreturn 0 }",
        ),
    ] {
        let errors = semantic_errors(program);
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, TypeError::InvalidBinaryOperator { op, .. } if op == "==")),
            "`==` on {name} operands must be rejected, got {errors:?}"
        );
    }
}

/// A newtype forwards its inner type's equality, so `==` on a newtype over a scalar keeps
/// compiling to the scalar compare.
#[test]
fn equality_on_a_newtype_over_a_scalar_is_accepted() {
    let errors = semantic_errors(
        r#"
        newtype Id = i32
        func main() -> i32 {
            val a = Id(1)
            val b = Id(2)
            if a == b { return 1 }
            return 0
        }
        "#,
    );
    assert!(
        errors.is_empty(),
        "a newtype over i32 compares like its inner type, got {errors:?}"
    );
}
