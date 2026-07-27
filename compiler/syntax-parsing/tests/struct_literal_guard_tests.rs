// The struct-literal guard in guarded headers (`if` / `while` / `for` / `match`).
//
// A bare `Ident {` in a header is the header's body block, never a struct literal.
// Inside a delimiter pair — `(...)` or `[...]` — that ambiguity is gone, so a struct
// literal must be accepted there even though the enclosing header is guarded.

use syntax_parsing::{parse, parse_expr, Expr};

/// Parse `src` as a whole program, expecting success.
fn parse_ok(src: &str) {
    if let Err(err) = parse(src) {
        panic!("expected `{src}` to parse, got: {err:?}");
    }
}

#[test]
fn struct_literal_allowed_in_match_scrutinee_call_argument() {
    let expr = parse_expr("match grid.get(Point { x: 3, y: 4 }) { _ => 1 }")
        .expect("struct literal inside the scrutinee's call arguments should parse");
    let Expr::Match { scrutinee, .. } = expr else {
        panic!("expected a match expression");
    };
    let Expr::Call { args, .. } = *scrutinee else {
        panic!("expected the scrutinee to be a call");
    };
    assert!(matches!(args.as_slice(), [Expr::StructLiteral { .. }]));
}

#[test]
fn struct_literal_allowed_in_guarded_header_call_argument() {
    parse_ok(
        r#"
        func test() {
            if contains(Point { x: 1, y: 2 }) { }
            while contains(Point { x: 1, y: 2 }) { }
            for p in points(Point { x: 1, y: 2 }) { }
            for i in 0..count(Point { x: 1, y: 2 }) { }
        }
    "#,
    );
}

#[test]
fn struct_literal_allowed_in_guarded_header_brackets() {
    parse_ok(
        r#"
        func test() {
            if flags[index(Point { x: 1, y: 2 })] { }
            for p in [Point { x: 1, y: 2 }] { }
        }
    "#,
    );
}

#[test]
fn struct_literal_allowed_in_parenthesized_guarded_header() {
    parse_ok(
        r#"
        func test() {
            if (Point { x: 1, y: 2 }).x > 0 { }
            while (Point { x: 1, y: 2 }).x > 0 { }
        }
    "#,
    );
}

#[test]
fn enum_struct_variant_allowed_in_guarded_header_call_argument() {
    // The same guard suppresses `Enum::Variant { .. }` construction; a delimiter
    // pair must lift it there too.
    parse_ok(
        r#"
        func test() {
            if accepts(Shape::Circle { radius: 2 }) { }
        }
    "#,
    );
}

#[test]
fn guarded_header_still_reads_bare_brace_as_the_body() {
    // The guard itself must survive: `if x {` is a condition plus a body block,
    // and `for p in items {` is an iterable plus a body block.
    parse_ok(
        r#"
        func test() {
            if flag { }
            while flag { }
            for p in items { }
            for i in 0..n { }
        }
    "#,
    );

    let expr = parse_expr("match value { _ => 1 }").expect("bare match scrutinee should parse");
    let Expr::Match {
        scrutinee, arms, ..
    } = expr
    else {
        panic!("expected a match expression");
    };
    assert!(matches!(*scrutinee, Expr::Identifier(_)));
    assert_eq!(arms.len(), 1);
}

#[test]
fn guard_is_restored_after_a_nested_delimiter_pair() {
    // After the argument list closes, the header is guarded again — so the
    // trailing `{` is the body, not a struct literal on `flag`.
    parse_ok(
        r#"
        func test() {
            if check(Point { x: 1, y: 2 }) && flag { }
            while check(Point { x: 1, y: 2 }) && flag { }
        }
    "#,
    );
}
