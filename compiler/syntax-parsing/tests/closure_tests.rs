// Closure literal and function-type parsing tests (§3.12)

use syntax_parsing::{parse_expr, Expr, Type};

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
