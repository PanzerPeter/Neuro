// Parsing tests for the pipeline operator `|>`.
//
// `|>` has no AST node: the parser rewrites `value |> target` into the call the
// target already stands for, so these assert on the *shape* the desugar produces
// rather than on an operator variant. Every property here is one a later stage
// would otherwise have to know about the operator and does not.

use shared_types::Literal;
use syntax_parsing::{parse_expr, BinaryOp, Expr, Stmt};

/// The callee and single argument of a desugared pipeline stage.
fn call_parts(expr: &Expr) -> (&Expr, &Expr) {
    match expr {
        Expr::Call { func, args, .. } => {
            assert_eq!(
                args.len(),
                1,
                "a pipeline stage passes exactly one argument"
            );
            (func, &args[0])
        }
        other => panic!("expected a call, got {:?}", other),
    }
}

fn ident_name(expr: &Expr) -> &str {
    match expr {
        Expr::Identifier(ident) => &ident.name,
        other => panic!("expected an identifier, got {:?}", other),
    }
}

#[test]
fn a_pipeline_into_a_name_is_a_plain_call() {
    let expr = parse_expr("x |> f").expect("a pipeline into a name should parse");
    let (func, arg) = call_parts(&expr);
    assert_eq!(ident_name(func), "f");
    assert_eq!(ident_name(arg), "x");
}

#[test]
fn a_chain_associates_left_to_right() {
    // `x |> f |> g` is `g(f(x))`, not `f(g(x))`: the outermost call is the last stage.
    let expr = parse_expr("x |> f |> g").expect("a chain should parse");
    let (outer_func, outer_arg) = call_parts(&expr);
    assert_eq!(ident_name(outer_func), "g");
    let (inner_func, inner_arg) = call_parts(outer_arg);
    assert_eq!(ident_name(inner_func), "f");
    assert_eq!(ident_name(inner_arg), "x");
}

#[test]
fn a_bound_method_becomes_a_method_call() {
    let expr = parse_expr("x |> model.forward").expect("a bound method should parse");
    let (func, arg) = call_parts(&expr);
    match func {
        Expr::FieldAccess { object, field, .. } => {
            assert_eq!(ident_name(object), "model");
            assert_eq!(field.name, "forward");
        }
        other => panic!("expected a field access callee, got {:?}", other),
    }
    assert_eq!(ident_name(arg), "x");
}

#[test]
fn an_associated_path_becomes_an_associated_call() {
    let expr = parse_expr("x |> Wrapper::of").expect("an associated path should parse");
    let (func, _) = call_parts(&expr);
    match func {
        Expr::Path {
            type_name, member, ..
        } => {
            assert_eq!(type_name.name, "Wrapper");
            assert_eq!(member.name, "of");
        }
        other => panic!("expected a path callee, got {:?}", other),
    }
}

#[test]
fn the_operator_binds_looser_than_arithmetic() {
    // Appendix B row 17: `a + b |> f` is `f(a + b)`, never `a + f(b)`.
    let expr = parse_expr("a + b |> f").expect("a pipeline after a sum should parse");
    let (func, arg) = call_parts(&expr);
    assert_eq!(ident_name(func), "f");
    match arg {
        Expr::Binary { op, .. } => assert_eq!(*op, BinaryOp::Add),
        other => panic!("expected the whole sum as the argument, got {:?}", other),
    }
}

#[test]
fn a_leading_operator_continues_the_previous_line() {
    // The multi-line chain form: the newline precedes the `|>`.
    let expr = parse_expr("data\n    |> normalize\n    |> augment")
        .expect("a multi-line chain should parse");
    let (func, arg) = call_parts(&expr);
    assert_eq!(ident_name(func), "augment");
    let (inner_func, inner_arg) = call_parts(arg);
    assert_eq!(ident_name(inner_func), "normalize");
    assert_eq!(ident_name(inner_arg), "data");
}

#[test]
fn a_closure_target_is_bound_to_a_temporary_first() {
    // A call whose callee is a closure *literal* is not a shape any later stage
    // accepts, so the parser binds it and calls the binding.
    let expr = parse_expr("x |> (|y: i32| -> i32 { y })")
        .expect("a parenthesized closure target should parse");
    let Expr::Block { stmts, .. } = &expr else {
        panic!("expected a block, got {:?}", expr);
    };
    assert_eq!(stmts.len(), 2);
    let name = match &stmts[0] {
        Stmt::VarDecl {
            name,
            init,
            mutable,
            ..
        } => {
            assert!(!mutable, "the pipeline temporary is immutable");
            assert!(matches!(init, Some(Expr::Closure { .. })));
            name.name.clone()
        }
        other => panic!("expected the temporary's declaration, got {:?}", other),
    };
    match &stmts[1] {
        Stmt::Expr(call) => {
            let (func, arg) = call_parts(call);
            assert_eq!(ident_name(func), name);
            assert_eq!(ident_name(arg), "x");
        }
        other => panic!("expected the call, got {:?}", other),
    }
}

#[test]
fn two_closure_targets_get_distinct_temporaries() {
    let expr = parse_expr("x |> (|y: i32| -> i32 { y }) |> (|z: i32| -> i32 { z })")
        .expect("two closure stages should parse");
    let Expr::Block { stmts, .. } = &expr else {
        panic!("expected a block, got {:?}", expr);
    };
    let Stmt::VarDecl { name: outer, .. } = &stmts[0] else {
        panic!("expected the outer temporary");
    };
    let Stmt::Expr(call) = &stmts[1] else {
        panic!("expected the outer call");
    };
    let (_, arg) = call_parts(call);
    let Expr::Block { stmts: inner, .. } = arg else {
        panic!("expected the inner stage's block, got {:?}", arg);
    };
    let Stmt::VarDecl { name: nested, .. } = &inner[0] else {
        panic!("expected the inner temporary");
    };
    assert_ne!(outer.name, nested.name);
}

#[test]
fn a_redundantly_parenthesized_name_is_still_a_call() {
    let expr = parse_expr("x |> (f)").expect("a parenthesized name should parse");
    let (func, _) = call_parts(&expr);
    assert_eq!(ident_name(func), "f");
}

#[test]
fn a_literal_target_is_rejected() {
    let err = parse_expr("x |> 2").expect_err("a literal is not a function value");
    assert!(
        err.to_string().contains("must be a function value"),
        "unexpected diagnostic: {err}"
    );
}

#[test]
fn an_applied_call_target_is_rejected() {
    // `x |> f(a)` is the mistake the diagnostic exists for: it reads as if the pipe
    // supplied the remaining argument, which the language does not define.
    let err = parse_expr("x |> f(a)").expect_err("an applied call is not a function value");
    assert!(
        err.to_string().contains("must be a function value"),
        "unexpected diagnostic: {err}"
    );
}

#[test]
fn a_pipeline_may_carry_a_literal_value() {
    let expr = parse_expr("5 |> f").expect("a literal value should parse");
    let (_, arg) = call_parts(&expr);
    assert!(matches!(arg, Expr::Literal(Literal::Integer(5, _), _)));
}
