// Struct shorthand + functional-update syntax (§3.3, Phase 2A)
// `Point { x, y }` shorthand and `Point { x: 1.0, ..p }` update syntax.
mod common;
use common::CompileTest;

// ── AC1: field-init shorthand binds the same-named value in scope ─────────────

#[test]
fn shorthand_fields_bind_locals() {
    let test = CompileTest::new();
    let source = r#"
struct Point {
    x: i32,
    y: i32
}

func main() -> i32 {
    val x = 7
    val y = 3
    val p = Point { x, y }
    return p.x - p.y
}
"#;
    let exit_code = test
        .compile_and_run("shorthand_fields.nr", source)
        .expect("shorthand struct literal should compile and run");
    assert_eq!(exit_code, 4, "7 - 3 should equal 4");
}

#[test]
fn shorthand_mixed_with_explicit() {
    let test = CompileTest::new();
    let source = r#"
struct Pair {
    a: i32,
    b: i32
}

func main() -> i32 {
    val a = 10
    val p = Pair { a, b: 5 }
    return p.a + p.b
}
"#;
    let exit_code = test
        .compile_and_run("shorthand_mixed.nr", source)
        .expect("mixed shorthand/explicit literal should compile and run");
    assert_eq!(exit_code, 15, "10 + 5 should equal 15");
}

// ── AC2: functional update copies unlisted fields from the base ──────────────

#[test]
fn update_overrides_one_field() {
    let test = CompileTest::new();
    // y is taken from `base`, x is overridden.
    let source = r#"
struct Point {
    x: i32,
    y: i32
}

func main() -> i32 {
    val base = Point { x: 1, y: 9 }
    val moved = Point { x: 4, ..base }
    return moved.x + moved.y
}
"#;
    let exit_code = test
        .compile_and_run("update_override.nr", source)
        .expect("functional update should compile and run");
    assert_eq!(exit_code, 13, "x overridden to 4, y inherited as 9 → 13");
}

#[test]
fn update_only_base_copies_all_fields() {
    let test = CompileTest::new();
    let source = r#"
struct Point {
    x: i32,
    y: i32
}

func main() -> i32 {
    val base = Point { x: 6, y: 8 }
    val copy = Point { ..base }
    return copy.x + copy.y
}
"#;
    let exit_code = test
        .compile_and_run("update_copy_all.nr", source)
        .expect("base-only update should compile and run");
    assert_eq!(exit_code, 14, "6 + 8 should equal 14");
}

// ── AC3: a base means omitted fields are NOT a missing-field error ────────────

#[test]
fn update_with_omitted_fields_is_accepted() {
    let test = CompileTest::new();
    let source = r#"
struct Config {
    host: i32,
    port: i32,
    timeout: i32
}

func main() -> i32 {
    val base = Config { host: 1, port: 2, timeout: 3 }
    val tuned = Config { port: 20, ..base }
    return tuned.host + tuned.port + tuned.timeout
}
"#;
    let exit_code = test
        .compile_and_run("update_omitted.nr", source)
        .expect("update with omitted fields should compile and run");
    assert_eq!(exit_code, 24, "1 + 20 + 3 should equal 24");
}

// ── AC4: diagnostics ─────────────────────────────────────────────────────────

#[test]
fn shorthand_undefined_identifier_is_rejected() {
    let test = CompileTest::new();
    // `y` is not bound, so the shorthand has no value to reference.
    let source = r#"
struct Point {
    x: i32,
    y: i32
}

func main() -> i32 {
    val x = 1
    val p = Point { x, y }
    return 0
}
"#;
    let source_path = test.write_source("shorthand_undefined.nr", source);
    let result = test.compile(&source_path);
    assert!(
        result.is_err(),
        "shorthand referencing an undefined name should fail"
    );
}

#[test]
fn update_base_of_wrong_struct_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
struct Point {
    x: i32,
    y: i32
}

struct Other {
    a: i32,
    b: i32
}

func main() -> i32 {
    val o = Other { a: 1, b: 2 }
    val p = Point { x: 5, ..o }
    return 0
}
"#;
    let source_path = test.write_source("update_wrong_base.nr", source);
    let result = test.compile(&source_path);
    assert!(
        result.is_err(),
        "functional-update base of the wrong struct type should fail"
    );
}

#[test]
fn missing_field_without_base_still_rejected() {
    let test = CompileTest::new();
    // No base present, so the missing `y` remains a hard error.
    let source = r#"
struct Point {
    x: i32,
    y: i32
}

func main() -> i32 {
    val p = Point { x: 1 }
    return 0
}
"#;
    let source_path = test.write_source("missing_no_base.nr", source);
    let result = test.compile(&source_path);
    assert!(
        result.is_err(),
        "missing field without a base should still fail"
    );
    let err = result.unwrap_err();
    assert!(
        err.contains("missing") || err.contains("MissingStructField"),
        "error should mention missing field, got: {}",
        err
    );
}
