// Parsing for `match` expressions and their patterns.

use syntax_parsing::{parse_expr, EnumPatternPayload, Expr, Pattern};

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
