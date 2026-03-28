// Struct type tests (Phase 2)
// Tests struct definition, instantiation, field access, and field mutation.
mod common;
use common::CompileTest;

// ── AC1/AC2: struct definition and instantiation compile end-to-end ──────────

#[test]
fn struct_definition_compiles() {
    let test = CompileTest::new();
    let source = r#"
struct Point {
    x: f64,
    y: f64
}

func main() -> i32 {
    val p = Point { x: 3.0, y: 4.0 }
    return 0
}
"#;
    let exit_code = test
        .compile_and_run("struct_definition.nr", source)
        .expect("struct definition and instantiation should compile and run");
    assert_eq!(exit_code, 0);
}

// ── AC3: field read resolves to the declared type ────────────────────────────

#[test]
fn field_read_i32_used_as_return() {
    let test = CompileTest::new();
    // Field value x = 7 used as the function return value.
    let source = r#"
struct Counter {
    x: i32,
    step: i32
}

func main() -> i32 {
    val c = Counter { x: 7, step: 1 }
    val value = c.x
    return value
}
"#;
    let exit_code = test
        .compile_and_run("field_read.nr", source)
        .expect("field read should compile and run");
    assert_eq!(exit_code, 7, "field x should equal 7");
}

#[test]
fn field_read_arithmetic() {
    let test = CompileTest::new();
    let source = r#"
struct Pair {
    a: i32,
    b: i32
}

func main() -> i32 {
    val p = Pair { a: 10, b: 3 }
    val result = p.a - p.b
    return result
}
"#;
    let exit_code = test
        .compile_and_run("field_arithmetic.nr", source)
        .expect("field arithmetic should compile and run");
    assert_eq!(exit_code, 7, "10 - 3 should equal 7");
}

// ── AC4a: field mutation accepted on mut binding ─────────────────────────────

#[test]
fn field_mutation_on_mut_binding() {
    let test = CompileTest::new();
    let source = r#"
struct Point {
    x: i32,
    y: i32
}

func main() -> i32 {
    mut origin = Point { x: 0, y: 0 }
    origin.x = 5
    return origin.x
}
"#;
    let exit_code = test
        .compile_and_run("field_mutation.nr", source)
        .expect("field mutation on mut binding should compile and run");
    assert_eq!(exit_code, 5, "after mutation, x should be 5");
}

// ── AC4b: field mutation rejected on immutable binding ───────────────────────

#[test]
fn field_mutation_on_immutable_binding_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
struct Point {
    x: i32,
    y: i32
}

func main() -> i32 {
    val p = Point { x: 1, y: 2 }
    p.x = 10
    return 0
}
"#;
    let source_path = test.write_source("immutable_field_mutation.nr", source);
    let result = test.compile(&source_path);
    assert!(
        result.is_err(),
        "mutating an immutable struct field should fail"
    );
    let err = result.unwrap_err();
    assert!(
        err.contains("immutable") || err.contains("AssignToImmutableField"),
        "error should mention immutability, got: {}",
        err
    );
}

// ── AC5: type errors — missing field ─────────────────────────────────────────

#[test]
fn struct_literal_missing_field_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
struct Point {
    x: f64,
    y: f64
}

func main() -> i32 {
    val p = Point { x: 1.0 }
    return 0
}
"#;
    let source_path = test.write_source("missing_field.nr", source);
    let result = test.compile(&source_path);
    assert!(
        result.is_err(),
        "missing struct field should cause a compile error"
    );
    let err = result.unwrap_err();
    assert!(
        err.contains("missing") || err.contains("MissingStructField"),
        "error should mention missing field, got: {}",
        err
    );
}

// ── AC5: type errors — unknown field ─────────────────────────────────────────

#[test]
fn struct_literal_unknown_field_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
struct Point {
    x: f64,
    y: f64
}

func main() -> i32 {
    val p = Point { x: 1.0, y: 2.0, z: 3.0 }
    return 0
}
"#;
    let source_path = test.write_source("unknown_field.nr", source);
    let result = test.compile(&source_path);
    assert!(
        result.is_err(),
        "unknown field in struct literal should cause a compile error"
    );
    let err = result.unwrap_err();
    assert!(
        err.contains("unknown") || err.contains("UnknownField") || err.contains("no field"),
        "error should mention unknown field, got: {}",
        err
    );
}

// ── AC5: type errors — wrong field type ──────────────────────────────────────

#[test]
fn struct_literal_wrong_field_type_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
struct Point {
    x: f64,
    y: f64
}

func main() -> i32 {
    val p = Point { x: true, y: 2.0 }
    return 0
}
"#;
    let source_path = test.write_source("wrong_field_type.nr", source);
    let result = test.compile(&source_path);
    assert!(
        result.is_err(),
        "wrong field type should cause a compile error"
    );
}

// ── Multiple structs in one file ──────────────────────────────────────────────

#[test]
fn multiple_structs_in_program() {
    let test = CompileTest::new();
    let source = r#"
struct Vec2 {
    x: f64,
    y: f64
}

struct Rect {
    width: i32,
    height: i32
}

func main() -> i32 {
    val v = Vec2 { x: 1.0, y: 2.0 }
    val r = Rect { width: 10, height: 5 }
    val area = r.width * r.height
    return area
}
"#;
    let exit_code = test
        .compile_and_run("multiple_structs.nr", source)
        .expect("multiple structs should compile and run");
    assert_eq!(exit_code, 50, "10 * 5 should equal 50");
}

// ── Struct defined after the function that uses it ───────────────────────────

#[test]
fn struct_defined_after_usage() {
    let test = CompileTest::new();
    // Structs are registered in a pre-pass, so definition order should not matter.
    let source = r#"
func main() -> i32 {
    val p = Score { value: 42 }
    return p.value
}

struct Score {
    value: i32
}
"#;
    let exit_code = test
        .compile_and_run("struct_after_usage.nr", source)
        .expect("struct defined after use should compile and run");
    assert_eq!(exit_code, 42);
}
