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
        type_args: Vec::new(),
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
        type_args: Vec::new(),
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
        type_args: Vec::new(),
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
        type_args: Vec::new(),
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
        type_args: Vec::new(),
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

/// `string + string` type-checks to `string` (§2.7 concatenation), distinct from
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

#[test]
fn enum_construction_forms_type_check() {
    // §3.5 — all three variant forms construct, and an enum flows as a binding,
    // a function parameter/return, and a struct field.
    let errors = semantic_errors(
        r#"
enum Color { Red, Green, Blue }
enum Shape { Circle { radius: f64 }, Rectangle { width: f64, height: f64 } }
enum Msg { Quit, Move(i32, i32) }

struct Tagged { kind: Color, n: i32 }

func make() -> Msg { Msg::Move(1, 2) }
func take(s: Shape) -> i32 { 0 }

func main() -> i32 {
    val c = Color::Red
    val s = Shape::Circle { radius: 5.0 }
    val r = Shape::Rectangle { width: 2.0, height: 3.0 }
    val m = make()
    val t = Tagged { kind: Color::Green, n: take(s) }
    return 0
}
"#,
    );
    assert!(errors.is_empty(), "valid enum program; got {errors:?}");
}

#[test]
fn unknown_enum_variant_is_rejected() {
    let errors = semantic_errors(
        r#"
enum Color { Red, Green }
func main() -> i32 {
    val c = Color::Blue
    0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::UnknownEnumVariant { .. })),
        "an undeclared variant must be rejected; got {errors:?}"
    );
}

#[test]
fn enum_variant_form_mismatch_is_rejected() {
    // A struct variant constructed with call syntax is a form error.
    let errors = semantic_errors(
        r#"
enum Shape { Circle { radius: f64 } }
func main() -> i32 {
    val s = Shape::Circle(5.0)
    0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::EnumVariantFormMismatch { .. })),
        "constructing a struct variant with `(...)` must be rejected; got {errors:?}"
    );
}

#[test]
fn enum_tuple_variant_arity_mismatch_is_rejected() {
    let errors = semantic_errors(
        r#"
enum Msg { Move(i32, i32) }
func main() -> i32 {
    val m = Msg::Move(1)
    0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::EnumVariantArityMismatch { .. })),
        "a tuple variant built with the wrong arity must be rejected; got {errors:?}"
    );
}

#[test]
fn enum_struct_variant_field_type_mismatch_is_rejected() {
    let errors = semantic_errors(
        r#"
enum Shape { Circle { radius: f64 } }
func main() -> i32 {
    val s = Shape::Circle { radius: true }
    0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::Mismatch { .. })),
        "a struct-variant field of the wrong type must be rejected; got {errors:?}"
    );
}

#[test]
fn non_scalar_enum_payload_is_rejected() {
    // §3.5 (Phase 1E) — payloads are scalar Copy primitives; a string payload is rejected.
    let errors = semantic_errors(
        r#"
enum Bad { Holds(string) }
func main() -> i32 { 0 }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::UnsupportedEnumPayload { .. })),
        "a non-scalar payload must be rejected; got {errors:?}"
    );
}

#[test]
fn match_all_pattern_forms_type_check() {
    // §3.6 — enum unit/tuple/struct variants, literal, or-pattern, range, guard,
    // and wildcard patterns all type-check in one exhaustive match.
    let errors = semantic_errors(
        r#"
enum Shape { Circle(i32), Rect { w: i32, h: i32 }, Unit }
func area(s: Shape) -> i32 {
    match s {
        Shape::Circle(r) => r * r,
        Shape::Rect { w, h } => w * h,
        Shape::Unit => 0
    }
}
func classify(n: i32) -> i32 {
    match n {
        0 => 1,
        1 | 2 => 2,
        3..=9 => 3,
        n if n < 0 => 4,
        _ => 9
    }
}
func main() -> i32 { area(Shape::Unit) + classify(5) }
"#,
    );
    assert!(errors.is_empty(), "valid match program; got {errors:?}");
}

#[test]
fn non_exhaustive_enum_match_is_rejected() {
    let errors = semantic_errors(
        r#"
enum E { A, B, C }
func f(e: E) -> i32 {
    match e {
        E::A => 1,
        E::B => 2
    }
}
func main() -> i32 { f(E::A) }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::NonExhaustiveMatch { .. })),
        "a match missing variant C must be rejected; got {errors:?}"
    );
}

#[test]
fn integer_match_without_wildcard_is_rejected() {
    let errors = semantic_errors(
        r#"
func f(n: i32) -> i32 {
    match n {
        0 => 1,
        1 => 2
    }
}
func main() -> i32 { f(0) }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::NonExhaustiveMatch { .. })),
        "an integer match needs a `_` arm; got {errors:?}"
    );
}

#[test]
fn match_arm_type_mismatch_is_rejected() {
    let errors = semantic_errors(
        r#"
func f(n: i32) -> i32 {
    match n {
        0 => 1,
        _ => true
    }
}
func main() -> i32 { f(0) }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::MatchArmTypeMismatch { .. })),
        "arms with incompatible body types must be rejected; got {errors:?}"
    );
}

#[test]
fn or_pattern_binding_is_rejected() {
    let errors = semantic_errors(
        r#"
func f(n: i32) -> i32 {
    match n {
        0 | x => x,
        _ => 0
    }
}
func main() -> i32 { f(0) }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::OrPatternBinding { .. })),
        "a binding in an or-pattern must be rejected; got {errors:?}"
    );
}

#[test]
fn match_on_unsupported_scrutinee_is_rejected() {
    let errors = semantic_errors(
        r#"
func f(s: string) -> i32 {
    match s {
        _ => 0
    }
}
func main() -> i32 { f("x") }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::UnsupportedMatchScrutinee { .. })),
        "matching on a string must be rejected in phase 1E; got {errors:?}"
    );
}
