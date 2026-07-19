// Mutable borrow tests (Phase 1.7)
// Verifies `&mut T` reference types, `&mut place` borrow expressions, the `*`
// dereference operator (read and write), and the borrow rules: `&mut` requires a
// `mut` binding, `*` applies only to references, and writing through `*` requires
// a `&mut`. Covers end-to-end compile+run and the rejection diagnostics.
mod common;
use common::CompileTest;

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

fn neurc_path() -> PathBuf {
    let neurc_exe = if cfg!(target_os = "windows") {
        "neurc.exe"
    } else {
        "neurc"
    };

    std::env::current_exe()
        .expect("Failed to get current exe path")
        .parent()
        .expect("Failed to get parent directory")
        .parent()
        .expect("Failed to get grandparent directory")
        .join(neurc_exe)
}

fn check_source(source: &str) -> (bool, String) {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let source_path = temp_dir.path().join("test.nr");
    fs::write(&source_path, source).expect("Failed to write source file");

    let output = Command::new(neurc_path())
        .arg("check")
        .arg(&source_path)
        .output()
        .expect("Failed to execute neurc check");

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (output.status.success(), stderr)
}

/// Compile + run, asserting the process exit code matches `expected`.
fn run_expecting(source: &str, expected: i32) {
    let test = CompileTest::new();
    let code = test
        .compile_and_run("test.nr", source)
        .expect("program should compile and run");
    assert_eq!(
        code, expected,
        "unexpected exit code for program:\n{source}"
    );
}

#[test]
fn mutate_through_a_mutable_reference() {
    // The canonical example: a void function mutates its `&mut i32` argument
    // in place; the change is visible at the caller's `mut` binding.
    let source = r#"
func increment(n: &mut i32) {
    *n = *n + 1
}
func main() -> i32 {
    mut counter: i32 = 40
    increment(&mut counter)
    increment(&mut counter)
    return counter
}
"#;
    run_expecting(source, 42);
}

#[test]
fn deref_read_through_local_reference() {
    let source = r#"
func main() -> i32 {
    mut x: i32 = 7
    val r: &mut i32 = &mut x
    *r = 35
    val v: i32 = *r
    return v
}
"#;
    run_expecting(source, 35);
}

#[test]
fn mutating_function_with_extra_argument() {
    let source = r#"
func add_into(n: &mut i32, delta: i32) {
    *n = *n + delta
}
func main() -> i32 {
    mut total: i32 = 10
    add_into(&mut total, 5)
    add_into(&mut total, 12)
    return total
}
"#;
    run_expecting(source, 27);
}

#[test]
fn mutably_borrowing_an_immutable_binding_is_rejected() {
    // `&mut` demands a `mut` binding.
    let source = r#"
func main() -> i32 {
    val x: i32 = 5
    val r: &mut i32 = &mut x
    return 0
}
"#;
    let (success, stderr) = check_source(source);
    assert!(!success, "&mut of a val should be rejected");
    assert!(
        stderr.contains("cannot mutably borrow"),
        "expected a mutable-borrow diagnostic, got: {stderr}"
    );
}

#[test]
fn dereferencing_a_non_reference_is_rejected() {
    let source = r#"
func main() -> i32 {
    val x: i32 = 5
    val y: i32 = *x
    return 0
}
"#;
    let (success, stderr) = check_source(source);
    assert!(!success, "deref of a non-reference should be rejected");
    assert!(
        stderr.contains("cannot dereference"),
        "expected a dereference diagnostic, got: {stderr}"
    );
}

#[test]
fn writing_through_an_immutable_reference_is_rejected() {
    let source = r#"
func main() -> i32 {
    mut x: i32 = 5
    val r: &i32 = &x
    *r = 9
    return 0
}
"#;
    let (success, stderr) = check_source(source);
    assert!(!success, "writing through &i32 should be rejected");
    assert!(
        stderr.contains("immutable reference"),
        "expected an immutable-reference diagnostic, got: {stderr}"
    );
}

#[test]
fn mutable_and_immutable_reference_types_are_distinct() {
    // There is no implicit `&mut T` -> `&T` coercion; passing a `&mut i32`
    // where a `&i32` is expected is a type mismatch.
    let source = r#"
func read(r: &i32) -> i32 { *r }
func main() -> i32 {
    mut x: i32 = 5
    val v: i32 = read(&mut x)
    return v
}
"#;
    let (success, stderr) = check_source(source);
    assert!(!success, "&mut i32 should not satisfy a &i32 parameter");
    assert!(
        stderr.contains("mismatch") || stderr.contains("&i32"),
        "expected a type-mismatch diagnostic, got: {stderr}"
    );
}

#[test]
fn deref_assignment_after_other_statements_parses() {
    // Regression: a `*r = v` statement following an expression-ending line must not
    // be glued to the previous expression as a multiplication continuation.
    let source = r#"
func main() -> i32 {
    mut x: i32 = 1
    val r: &mut i32 = &mut x
    *r = 99
    return *r
}
"#;
    run_expecting(source, 99);
}
