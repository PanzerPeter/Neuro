// Closure literal and function-type parsing tests

use ast_types::FunctionMode;
use syntax_parsing::{Expr, Type, parse_expr};

#[test]
fn parses_single_expression_closure() {
    let expr = parse_expr("|x: i32| x * x").expect("closure should parse");
    match expr {
        Expr::Closure {
            params,
            ret,
            is_move,
            ..
        } => {
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].name.name, "x");
            assert!(params[0].ty.is_some());
            assert!(ret.is_none());
            assert!(!is_move);
        }
        other => panic!("expected closure, got {:?}", other),
    }
}

#[test]
fn parses_move_closure_with_return_type() {
    let expr = parse_expr("move |a: i32, b: i32| -> i32 { a + b }").expect("closure should parse");
    match expr {
        Expr::Closure {
            params,
            ret,
            is_move,
            body,
            ..
        } => {
            assert_eq!(params.len(), 2);
            assert!(ret.is_some());
            assert!(is_move);
            assert!(matches!(*body, Expr::Block { .. }));
        }
        other => panic!("expected closure, got {:?}", other),
    }
}

#[test]
fn parses_zero_parameter_closure() {
    let expr = parse_expr("|| 42").expect("zero-parameter closure should parse");
    match expr {
        Expr::Closure { params, .. } => assert!(params.is_empty()),
        other => panic!("expected closure, got {:?}", other),
    }
}

#[test]
fn parses_function_type_annotation() {
    // A `(T) -> R` binding annotation is a function type, not a tuple.
    let expr = parse_expr("|f: (i32) -> i32| f").expect("function-typed parameter should parse");
    match expr {
        Expr::Closure { params, .. } => match params[0].ty.as_ref().expect("param has a type") {
            Type::Function { params, ret, .. } => {
                assert_eq!(params.len(), 1);
                assert!(matches!(**ret, Type::Named(_)));
            }
            other => panic!("expected function type, got {:?}", other),
        },
        other => panic!("expected closure, got {:?}", other),
    }
}

/// The function type a closure literal's one parameter is annotated with.
fn param_function_mode(source: &str) -> FunctionMode {
    match parse_expr(source).expect("closure should parse") {
        Expr::Closure { params, .. } => match params[0].ty.as_ref().expect("param has a type") {
            Type::Function { mode, .. } => *mode,
            other => panic!("expected function type, got {:?}", other),
        },
        other => panic!("expected closure, got {:?}", other),
    }
}

#[test]
fn a_function_type_carries_its_call_mode() {
    assert_eq!(
        param_function_mode("|f: (i32) -> i32| f"),
        FunctionMode::Shared
    );
    assert_eq!(
        param_function_mode("|f: mut (i32) -> i32| f"),
        FunctionMode::Mut
    );
    assert_eq!(
        param_function_mode("|f: once () -> string| f"),
        FunctionMode::Once
    );
}

#[test]
fn a_call_mode_needs_a_function_type_after_it() {
    assert!(parse_expr("|f: mut (i32, i32)| f").is_err());
    assert!(parse_expr("|f: once (i32, bool)| f").is_err());
}

/// `once` is a mode only directly before `(`; everywhere else it is an ordinary name.
#[test]
fn once_stays_an_ordinary_name() {
    let expr = parse_expr("|once: i32| once + 1").expect("`once` names a parameter");
    assert!(matches!(expr, Expr::Closure { .. }));
}
