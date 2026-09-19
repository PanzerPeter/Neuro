// Parsing tests for the function composition operator `>>`.
//
// `>>` is two adjacent `>` tokens rather than one token of its own, so half of what
// matters here is what the operator does NOT claim: a comparison, and the closing
// angle brackets of a nested generic type. The rest is the shape the parser produces:
// a flat `Expr::Compose` chain, an applied composition folded into nested calls, and
// a rejection for any operand that is not a name.

use syntax_parsing::{parse, parse_expr, BinaryOp, Expr};

fn compose_names(expr: &Expr) -> Vec<String> {
    match expr {
        Expr::Compose { functions, .. } => functions.iter().map(|f| f.name.clone()).collect(),
        other => panic!("expected a composition, got {:?}", other),
    }
}

fn call_parts(expr: &Expr) -> (&Expr, &Expr) {
    match expr {
        Expr::Call { func, args, .. } => {
            assert_eq!(args.len(), 1, "an applied stage takes one argument");
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
fn two_names_compose_into_one_node() {
    let expr = parse_expr("f >> g").expect("a composition should parse");
    assert_eq!(compose_names(&expr), vec!["f", "g"]);
}

#[test]
fn a_chain_is_flattened_left_to_right() {
    // `f >> g >> h` is one node in application order, not a tree: a nested chain
    // would make the composed closure call a closure, which is a capture.
    let expr = parse_expr("f >> g >> h").expect("a chain should parse");
    assert_eq!(compose_names(&expr), vec!["f", "g", "h"]);
}

#[test]
fn parentheses_around_an_operand_are_peeled() {
    let expr = parse_expr("(f) >> (g >> h)").expect("a parenthesized chain should parse");
    assert_eq!(compose_names(&expr), vec!["f", "g", "h"]);
}

#[test]
fn an_applied_composition_becomes_nested_calls() {
    // `(f >> g)(x)` is `g(f(x))`: a chain called where it is written needs no
    // function value at all.
    let expr = parse_expr("(f >> g)(x)").expect("an applied composition should parse");
    let (outer, inner) = call_parts(&expr);
    assert_eq!(ident_name(outer), "g");
    let (inner_func, arg) = call_parts(inner);
    assert_eq!(ident_name(inner_func), "f");
    assert_eq!(ident_name(arg), "x");
}

#[test]
fn a_pipeline_applies_the_composition_it_carries() {
    // Appendix B rows 16 and 17: `>>` binds tighter, so `x |> f >> g` is
    // `x |> (f >> g)` and not `(x |> f) >> g`.
    let expr = parse_expr("x |> f >> g").expect("a piped composition should parse");
    let (outer, inner) = call_parts(&expr);
    assert_eq!(ident_name(outer), "g");
    let (inner_func, arg) = call_parts(inner);
    assert_eq!(ident_name(inner_func), "f");
    assert_eq!(ident_name(arg), "x");
}

#[test]
fn a_chain_may_break_the_line_before_the_operator() {
    let expr = parse_expr("f\n    >> g\n    >> h").expect("a multi-line chain should parse");
    assert_eq!(compose_names(&expr), vec!["f", "g", "h"]);
}

#[test]
fn a_spaced_pair_of_angle_brackets_is_not_the_operator() {
    // `a > > b` is a comparison followed by a stray `>`, and must not be read as
    // composition: adjacency is what makes the pair one operator.
    assert!(parse_expr("a > > b").is_err());
}

#[test]
fn a_comparison_is_left_alone() {
    let expr = parse_expr("a > b").expect("a comparison should parse");
    match expr {
        Expr::Binary { op, .. } => assert_eq!(op, BinaryOp::Greater),
        other => panic!("expected a comparison, got {:?}", other),
    }
}

#[test]
fn nested_generic_arguments_still_close() {
    // The reason `>>` is not a lexer token: `Option<Option<i32>>` ends in two `>`
    // that belong to the type, one per open bracket.
    let source = r#"
func main() -> i32 {
    val nested: Option<Option<i32>> = Option::None
    0
}
"#;
    assert!(parse(source).is_ok(), "a nested generic type should parse");
}

#[test]
fn a_turbofish_still_closes() {
    let source = r#"
func main() -> i32 {
    val v = identity::<Option<i32>>(Option::None)
    0
}
"#;
    assert!(parse(source).is_ok(), "a nested turbofish should parse");
}

#[test]
fn a_non_name_operand_is_rejected_at_the_operator() {
    // Left to fall through, `1 >> 2` would be a comparison against a stray `>`,
    // which says nothing about composition.
    let error = parse_expr("1 >> 2").expect_err("a literal operand should be rejected");
    assert!(
        error.to_string().contains("the operands of `>>`"),
        "the diagnostic should name the operator, got {}",
        error
    );
}

#[test]
fn a_closure_literal_operand_is_rejected() {
    let error =
        parse_expr("f >> (|x: i32| -> i32 { x })").expect_err("a closure operand is rejected");
    assert!(error.to_string().contains("the operands of `>>`"));
}

#[test]
fn a_bound_method_operand_is_rejected() {
    let error = parse_expr("f >> model.forward").expect_err("a bound method is rejected");
    assert!(error.to_string().contains("the operands of `>>`"));
}
