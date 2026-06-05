// Neuro Programming Language - Semantic Analysis
// Integration tests: Extended integer types and width/sign mismatches

use semantic_analysis::{type_check, TypeError};

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
    assert!(errors
        .iter()
        .any(|e| matches!(e, TypeError::Mismatch { .. })));
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
    assert!(errors
        .iter()
        .any(|e| matches!(e, TypeError::Mismatch { .. })));
}

#[test]
fn error_unsigned_with_float() {
    let source = r#"func test(a: u32, b: f32) -> u32 {
        return a + b
    }"#;
    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);
    assert!(result.is_err(), "u32 + f32 should fail type check");
}
