// Parsing for `match` expressions and their patterns.

use syntax_parsing::{EnumPatternPayload, Expr, Pattern, parse_expr};

/// Parse `src` as a single expression, expecting a `match`.
fn parse_match(src: &str) -> Expr {
    parse_expr(src).expect("match expression should parse")
}

#[test]
fn parses_enum_value_and_range_patterns() {
    let expr = parse_match(
        r#"match n {
    Color::Red => 1,
    Maybe::Some(x) => x,
    Shape::Circle { radius } => radius,
    0 => 2,
    1 | 2 => 3,
    3..=9 => 4,
    n if n < 0 => 5,
    _ => 9
}"#,
    );
    let Expr::Match { arms, .. } = expr else {
        panic!("expected a match expression");
    };
    assert_eq!(arms.len(), 8);

    // Unit enum variant.
    assert!(matches!(
        &arms[0].patterns[0],
        Pattern::Enum {
            payload: EnumPatternPayload::Unit,
            ..
        }
    ));
    // Tuple variant binding.
    assert!(matches!(
        &arms[1].patterns[0],
        Pattern::Enum { payload: EnumPatternPayload::Tuple(subs), .. } if subs.len() == 1
    ));
    // Struct variant binding.
    assert!(matches!(
        &arms[2].patterns[0],
        Pattern::Enum { payload: EnumPatternPayload::Struct(fields), .. } if fields.len() == 1
    ));
    // Literal.
    assert!(matches!(&arms[3].patterns[0], Pattern::Literal(..)));
    // Or-pattern: two alternatives.
    assert_eq!(arms[4].patterns.len(), 2);
    // Inclusive range.
    assert!(matches!(
        &arms[5].patterns[0],
        Pattern::Range {
            inclusive: true,
            ..
        }
    ));
    // Guarded bare binding.
    assert!(matches!(&arms[6].patterns[0], Pattern::Binding(_)));
    assert!(arms[6].guard.is_some());
    // Wildcard.
    assert!(matches!(&arms[7].patterns[0], Pattern::Wildcard(_)));
}

#[test]
fn parses_negative_literal_and_exclusive_range() {
    let expr = parse_match("match n { -5..0 => 1, _ => 0 }");
    let Expr::Match { arms, .. } = expr else {
        panic!("expected a match expression");
    };
    assert!(matches!(
        &arms[0].patterns[0],
        Pattern::Range {
            inclusive: false,
            ..
        }
    ));
}

fn first_pattern(src: &str) -> Pattern {
    let Expr::Match { arms, .. } = parse_match(src) else {
        panic!("expected a match");
    };
    arms[0].patterns[0].clone()
}

#[test]
fn parses_an_at_binding_around_any_pattern() {
    let Pattern::At { name, pattern, .. } = first_pattern("match n { x @ 1..=9 => x, _ => 0 }")
    else {
        panic!("expected an `@` binding");
    };
    assert_eq!(name.name, "x");
    assert!(matches!(*pattern, Pattern::Range { .. }));

    let nested = first_pattern("match n { Maybe::Some(v @ Tok::End) => 1, _ => 0 }");
    let Pattern::Enum {
        payload: EnumPatternPayload::Tuple(subs),
        ..
    } = nested
    else {
        panic!("expected a payload");
    };
    assert!(matches!(subs[0], Pattern::At { .. }));
}

#[test]
fn parses_array_patterns_with_and_without_a_rest() {
    let Pattern::Array { elements, rest, .. } =
        first_pattern("match s { [1, x, ..tail] => 0, _ => 1 }")
    else {
        panic!("expected an array pattern");
    };
    assert_eq!(elements.len(), 2);
    let binding = rest.and_then(|r| r.binding).expect("a named rest");
    assert_eq!(binding.name, "tail");

    let Pattern::Array { elements, rest, .. } = first_pattern("match s { [] => 0, _ => 1 }") else {
        panic!("expected an array pattern");
    };
    assert!(elements.is_empty() && rest.is_none());

    let Pattern::Array { elements, rest, .. } = first_pattern("match s { [a, ..,] => 0, _ => 1 }")
    else {
        panic!("expected an array pattern");
    };
    assert_eq!(elements.len(), 1);
    assert!(rest.is_some_and(|r| r.binding.is_none()));
}

#[test]
fn a_rest_closes_an_array_pattern() {
    for src in ["match s { [.., z] => 0 }", "match s { [.., ..] => 0 }"] {
        let err = parse_expr(src).expect_err("a rest must come last");
        assert!(err.to_string().contains("no elements after"), "{err}");
    }
}

#[test]
fn a_struct_variant_pattern_may_end_in_dots() {
    let Pattern::Enum {
        payload: EnumPatternPayload::Struct(fields),
        ..
    } = first_pattern("match s { Shape::Rect { w, .. } => w, Shape::Dot { .. } => 0 }")
    else {
        panic!("expected a struct payload");
    };
    assert_eq!(fields.len(), 1);
}
