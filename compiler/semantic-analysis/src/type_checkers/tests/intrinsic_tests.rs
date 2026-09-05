use super::super::*;
use super::*;

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
        arg_labels: Vec::new(),
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
fn int_intrinsic_call(method: &str, recv: i128, arg: i128) -> Expr {
    Expr::Call {
        func: Box::new(Expr::FieldAccess {
            object: Box::new(Expr::Literal(
                Literal::Integer(recv, Some(shared_types::IntSuffix::U8)),
                Span::new(0, 5),
            )),
            field: make_ident(method),
            span: Span::new(0, 20),
        }),
        arg_labels: Vec::new(),
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

/// A program body wrapped in a `main` that also declares `Option`, which the checker
/// normally receives from the prelude `neurc` prepends.
fn program_with_option(body: &str) -> String {
    format!("enum Option<T> {{ Some(T), None }}\nfunc main() -> i32 {{\n{body}\n}}\n")
}

#[test]
fn checked_intrinsics_resolve_to_an_option_instance() {
    for method in ["checked_add", "checked_sub", "checked_mul"] {
        let errors = semantic_errors(&program_with_option(&format!(
            "    val a: u8 = 200
    val r: Option<u8> = a.{method}(100u8)
    return match r {{ Option::Some(v) => v as i32, Option::None => 0 }}"
        )));
        assert!(
            errors.is_empty(),
            "{method} should type-check to Option<u8>, got: {errors:?}"
        );
    }
}

#[test]
fn checked_intrinsic_mismatched_option_instance_rejected() {
    // The receiver is `u8`, so the result is `Option<u8>` — not `Option<i64>`.
    let errors = semantic_errors(&program_with_option(
        "    val a: u8 = 200
    val r: Option<i64> = a.checked_add(100u8)
    return 0",
    ));
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::Mismatch { .. })),
        "Expected Mismatch, got: {errors:?}"
    );
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
        arg_labels: Vec::new(),
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
        arg_labels: Vec::new(),
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
        arg_labels: Vec::new(),
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
fn is_nan_resolves_to_bool_on_both_full_precision_floats() {
    let errors = semantic_errors(
        "func main() -> i32 {
    val d: f64 = 0.0 / 0.0
    val s: f32 = 1.5f32
    val a: bool = d.is_nan()
    val b: bool = s.is_nan()
    if a { return 1 }
    if b { return 2 }
    return 0
}",
    );
    assert!(errors.is_empty(), "is_nan should type-check: {errors:?}");
}

#[test]
fn is_nan_wrong_arity_rejected() {
    let errors = semantic_errors(
        "func main() -> i32 {
    val d: f64 = 1.0
    val a: bool = d.is_nan(2.0)
    if a { return 1 }
    return 0
}",
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::ArgumentCountMismatch { .. })),
        "Expected ArgumentCountMismatch, got: {errors:?}"
    );
}

#[test]
fn is_nan_on_non_float_receiver_reports_method_not_found() {
    // Integers cannot be NaN, and half-precision has no scalar arithmetic contract
    // that could produce one — both fall through to the ordinary method lookup.
    for decl in [
        "val x: i32 = 1",
        "val x: f16 = 1.5f16",
        "val x: bf16 = 1.5bf16",
    ] {
        let errors = semantic_errors(&format!(
            "func main() -> i32 {{
    {decl}
    val a: bool = x.is_nan()
    if a {{ return 1 }}
    return 0
}}"
        ));
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, TypeError::MethodNotFound { .. })),
            "Expected MethodNotFound for `{decl}`, got: {errors:?}"
        );
    }
}
