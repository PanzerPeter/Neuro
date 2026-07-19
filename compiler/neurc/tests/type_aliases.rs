// Type-alias tests
// End-to-end coverage: a `type Name = Target` alias is transparent and resolves
// to its target in every type position, chains collapse to the ultimate target,
// and malformed alias sets are rejected at compile time.
mod common;
use common::CompileTest;

// ── Alias resolves in a local annotation and behaves as the target type ───────

#[test]
fn alias_of_primitive_runs_as_target() {
    let test = CompileTest::new();
    let source = r#"
type Count = i32

func main() -> i32 {
    val n: Count = 42
    return n
}
"#;
    let exit_code = test
        .compile_and_run("alias_primitive.nr", source)
        .expect("alias of i32 should compile and run");
    assert_eq!(exit_code, 42);
}

// ── Alias used across param, return, and cast positions ───────────────────────

#[test]
fn alias_in_param_return_and_cast() {
    let test = CompileTest::new();
    let source = r#"
type Id = i64

func to_id(raw: i32) -> Id {
    raw as Id
}

func main() -> i32 {
    val tag: Id = to_id(9)
    return tag as i32
}
"#;
    let exit_code = test
        .compile_and_run("alias_param_return_cast.nr", source)
        .expect("alias in param/return/cast should compile and run");
    assert_eq!(exit_code, 9);
}

// ── Alias as a struct field type ──────────────────────────────────────────────

#[test]
fn alias_as_struct_field_type() {
    let test = CompileTest::new();
    let source = r#"
type Meters = f64

struct Box {
    width: Meters
}

func main() -> i32 {
    val b = Box { width: 5.0 }
    return b.width as i32
}
"#;
    let exit_code = test
        .compile_and_run("alias_struct_field.nr", source)
        .expect("alias as struct field should compile and run");
    assert_eq!(exit_code, 5);
}

// ── Alias chains collapse to the ultimate target ──────────────────────────────

#[test]
fn alias_chain_resolves() {
    let test = CompileTest::new();
    let source = r#"
type A = B
type B = i32

func main() -> i32 {
    val x: A = 13
    return x
}
"#;
    let exit_code = test
        .compile_and_run("alias_chain.nr", source)
        .expect("alias chain should compile and run");
    assert_eq!(exit_code, 13);
}

// ── Malformed alias sets are rejected ─────────────────────────────────────────

#[test]
fn cyclic_alias_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
type A = B
type B = A

func main() -> i32 {
    return 0
}
"#;
    let path = test.write_source("alias_cycle.nr", source);
    let err = test
        .compile(&path)
        .expect_err("cyclic alias must not compile");
    assert!(
        err.contains("cyclic") || err.contains("itself"),
        "diagnostic should mention the cycle, got: {err}"
    );
}

#[test]
fn duplicate_alias_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
type A = i32
type A = f64

func main() -> i32 {
    return 0
}
"#;
    let path = test.write_source("alias_duplicate.nr", source);
    let err = test
        .compile(&path)
        .expect_err("duplicate alias must not compile");
    assert!(
        err.contains("duplicate type alias"),
        "diagnostic should mention the duplicate, got: {err}"
    );
}

#[test]
fn alias_shadowing_builtin_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
type i32 = f64

func main() -> i32 {
    return 0
}
"#;
    let path = test.write_source("alias_shadow.nr", source);
    let err = test
        .compile(&path)
        .expect_err("alias shadowing a built-in must not compile");
    assert!(
        err.contains("shadows a built-in"),
        "diagnostic should mention built-in shadowing, got: {err}"
    );
}
