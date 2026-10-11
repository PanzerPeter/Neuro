// Integration tests: Extended integer types and width/sign mismatches

use semantic_analysis::{TypeError, type_check};

#[test]
fn type_check_extended_integers_i8() {
    let source = r#"func test(a: i8, b: i8) -> i8 {
        return a + b
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(
        result.is_ok(),
        "i8 arithmetic should type check: {:?}",
        result
    );
}

#[test]
fn type_check_extended_integers_i16() {
    let source = r#"func test(a: i16, b: i16) -> i16 {
        val result: i16 = a * b
        return result
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(
        result.is_ok(),
        "i16 arithmetic should type check: {:?}",
        result
    );
}

#[test]
fn type_check_extended_integers_u8() {
    let source = r#"func test(x: u8) -> u8 {
        return x + x
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(
        result.is_ok(),
        "u8 arithmetic should type check: {:?}",
        result
    );
}

#[test]
fn type_check_extended_integers_u16() {
    let source = r#"func test(a: u16, b: u16) -> u16 {
        return a - b
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(
        result.is_ok(),
        "u16 arithmetic should type check: {:?}",
        result
    );
}

#[test]
fn type_check_extended_integers_u32() {
    let source = r#"func test(a: u32, b: u32) -> u32 {
        return a / b
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(
        result.is_ok(),
        "u32 arithmetic should type check: {:?}",
        result
    );
}

#[test]
fn type_check_extended_integers_u64() {
    let source = r#"func test(a: u64, b: u64) -> bool {
        return a > b
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(
        result.is_ok(),
        "u64 comparison should type check: {:?}",
        result
    );
}

#[test]
fn error_signed_unsigned_mismatch() {
    let source = r#"func test(a: i32, b: u32) -> i32 {
        return a + b
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(result.is_err(), "i32 + u32 should fail type check");
    let errors = result.unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::Mismatch { .. }))
    );
}

#[test]
fn error_different_width_mismatch() {
    let source = r#"func test(a: i8, b: i16) -> i8 {
        return a + b
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(result.is_err(), "i8 + i16 should fail type check");
    let errors = result.unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::Mismatch { .. }))
    );
}

#[test]
fn error_unsigned_with_float() {
    let source = r#"func test(a: u32, b: f32) -> u32 {
        return a + b
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(result.is_err(), "u32 + f32 should fail type check");
    let errors = result.unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::Mismatch { .. })),
        "{errors:?}"
    );
}

fn float_range_errors(source: &str) -> Vec<(semantic_analysis::Type, shared_types::Span)> {
    let items = syntax_parsing::parse(source).unwrap();
    match type_check(&items) {
        Ok(_) => Vec::new(),
        Err(errors) => errors
            .into_iter()
            .filter_map(|e| match e {
                TypeError::FloatLiteralOutOfRange { ty, span } => Some((ty, span)),
                _ => None,
            })
            .collect(),
    }
}

#[test]
fn regression_lim_209_float_literal_rounding_to_infinity_is_refused() {
    use semantic_analysis::Type;
    for (source, literal, ty) in [
        ("func f() -> f64 { 1e400 }", "1e400", Type::F64),
        ("func f() -> f32 { 1e39f32 }", "1e39f32", Type::F32),
        ("func f() -> f32 { 1e39 }", "1e39", Type::F32),
        ("func f() -> f16 { 65520.0 }", "65520.0", Type::F16),
        ("func f() -> bf16 { 3.4e38bf16 }", "3.4e38bf16", Type::BF16),
        ("func f() -> f64 { -1e400 }", "1e400", Type::F64),
    ] {
        let errors = float_range_errors(source);
        assert_eq!(errors.len(), 1, "{source}: {errors:?}");
        assert_eq!(errors[0].0, ty, "{source}");
        assert_eq!(
            &source[errors[0].1.start..errors[0].1.end],
            literal,
            "{source}"
        );
    }
    for source in [
        "func f() -> f64 { 1.7e308 }",
        "func f() -> f32 { 3.4e38 }",
        "func f() -> f16 { 65504.0 }",
        "func f() -> f16 { 65519.0 }",
        "func f() -> bf16 { 3.38e38 }",
        "func f() -> f32 { 1e-50 }",
    ] {
        assert!(
            float_range_errors(source).is_empty(),
            "{source} must type-check"
        );
    }
}
