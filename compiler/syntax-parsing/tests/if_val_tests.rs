// Parsing for `if val` and `while val`, which reach the AST as the `match` they mean.

use shared_types::Literal;
use syntax_parsing::{Expr, Item, Pattern, Stmt, parse};

/// The statements of the first function body in `source`.
fn first_fn_body(source: &str) -> Vec<Stmt> {
    let items = parse(source).expect("source should parse");
    for item in &items {
        if let Item::Function(func) = item {
            return func.body.clone();
        }
    }
    panic!("no function found");
}

/// The two arms of `expr`, which must be the `match` an `if val` or `while val` means:
/// the user's pattern with its block, then `_` with the other branch.
fn val_arms(expr: &Expr) -> (&Pattern, &[Stmt], &Expr) {
    let Expr::Match { arms, .. } = expr else {
        panic!("expected a match, got {expr:?}");
    };
    assert_eq!(
        arms.len(),
        2,
        "an `if val` is one pattern arm and a `_` arm"
    );
    assert!(matches!(
        arms[1].patterns.as_slice(),
        [Pattern::Wildcard(_)]
    ));
    let Expr::Block { stmts, .. } = arms[0].body.as_ref() else {
        panic!("the pattern arm's body is the block");
    };
    (&arms[0].patterns[0], stmts, &arms[1].body)
}

#[test]
fn if_val_with_else_is_a_two_arm_match() {
    let body = first_fn_body(
        r#"
        func test(o: Option<i32>) -> i32 {
            if val Some(x) = o { x } else { 0 }
        }
    "#,
    );
    let [Stmt::Expr(matched)] = body.as_slice() else {
        panic!("expected one expression statement, got {body:?}");
    };
    let (pattern, then, otherwise) = val_arms(matched);
    assert!(matches!(pattern, Pattern::UnqualifiedEnum { .. }));
    assert_eq!(then.len(), 1, "with an `else`, the block keeps its value");
    assert!(matches!(otherwise, Expr::Block { stmts, .. } if stmts.len() == 1));
}

#[test]
fn if_val_without_else_ends_its_block_in_unit() {
    let body = first_fn_body(
        r#"
        func test(o: Option<i32>) {
            if val Some(x) = o { bump(x) }
        }
    "#,
    );
    let [Stmt::Expr(matched)] = body.as_slice() else {
        panic!("expected one expression statement, got {body:?}");
    };
    let (_, then, otherwise) = val_arms(matched);
    assert!(
        matches!(then.last(), Some(Stmt::Expr(Expr::Block { stmts, .. })) if stmts.is_empty()),
        "the discarded `bump(x)` is followed by `{{}}`, got {then:?}"
    );
    assert!(matches!(otherwise, Expr::Block { stmts, .. } if stmts.is_empty()));
}

#[test]
fn else_if_val_becomes_the_else_branch_of_a_plain_if() {
    let body = first_fn_body(
        r#"
        func test(c: bool, o: Option<i32>) -> i32 {
            if c { 1 } else if !c { 2 } else if val Some(x) = o { x } else { 3 }
        }
    "#,
    );
    let [
        Stmt::If {
            else_if_blocks,
            else_block: Some(rest),
            ..
        },
    ] = body.as_slice()
    else {
        panic!("expected a plain `if` at the head, got {body:?}");
    };
    assert_eq!(else_if_blocks.len(), 1, "`else if !c` stays a plain link");
    let [Stmt::Expr(matched)] = rest.as_slice() else {
        panic!("the `if val` link is the else branch, got {rest:?}");
    };
    val_arms(matched);
}

#[test]
fn while_val_is_a_while_around_a_match_that_breaks() {
    let body = first_fn_body(
        r#"
        func test(v: Vec<i32>) {
            'drain: while val Some(x) = v.pop() { use(x) }
        }
    "#,
    );
    let [
        Stmt::While {
            label: Some(label),
            condition,
            body,
            ..
        },
    ] = body.as_slice()
    else {
        panic!("expected a while, got {body:?}");
    };
    assert_eq!(label.name, "drain");
    assert!(
        matches!(condition, Expr::Paren(inner, _)
            if matches!(inner.as_ref(), Expr::Literal(Literal::Boolean(true), _))),
        "the condition is `(true)`, which the `while true` lint does not flag"
    );
    let [Stmt::Expr(matched)] = body.as_slice() else {
        panic!("the loop body is the match, got {body:?}");
    };
    let (_, _, otherwise) = val_arms(matched);
    assert!(matches!(
        otherwise,
        Expr::Block { stmts, .. } if matches!(stmts.as_slice(), [Stmt::Break { label: None, value: None, .. }])
    ));
}

#[test]
fn an_if_val_scrutinee_is_not_read_as_a_struct_literal() {
    let body = first_fn_body(
        r#"
        func test(o: Option<i32>) {
            if val Some(x) = o { }
        }
    "#,
    );
    let [Stmt::Expr(Expr::Match { scrutinee, .. })] = body.as_slice() else {
        panic!("expected a match, got {body:?}");
    };
    assert!(matches!(scrutinee.as_ref(), Expr::Identifier(id) if id.name == "o"));
}

#[test]
fn a_while_val_head_without_equals_is_an_error() {
    let message = parse(
        r#"
        func test(v: Vec<i32>) {
            while val Some(x) v.pop() { }
        }
    "#,
    )
    .expect_err("the `=` is required")
    .to_string();
    assert!(
        message.contains("'=' after a `while val` pattern"),
        "got: {message}"
    );
}
