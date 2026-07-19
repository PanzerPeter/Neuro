// Immutable borrow tests (Phase 1.7)
// Verifies `&T` reference types and `&place` borrow expressions: borrowing does
// not move the borrowee, references are Copy, and method/field access auto-derefs
// through a borrow. Covers end-to-end compile+run and the borrow-a-temporary error.
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
fn borrowing_a_string_does_not_move_it() {
    // The canonical example: `length(&msg)` borrows; `msg` stays usable.
    let source = r#"
func describe(s: &string) -> u64 {
    s.len()
}
func main() -> i32 {
    val msg: string = "Neuro"
    val n: u64 = describe(&msg)
    val again: u64 = msg.len()
    return (n as i32) + (again as i32)
}
"#;
    // "Neuro" is 5 bytes; borrowed twice → 5 + 5 = 10.
    run_expecting(source, 10);
}

#[test]
fn use_after_borrow_is_allowed() {
    // A borrow must not be flagged as a move by the borrow checker.
    let source = r#"
func describe(s: &string) -> u64 {
    s.len()
}
func main() -> i32 {
    val msg: string = "hello"
    val n: u64 = describe(&msg)
    val m: u64 = describe(&msg)
    return 0
}
"#;
    let (success, stderr) = check_source(source);
    assert!(success, "borrowing must not move; got: {stderr}");
}

#[test]
fn clone_through_a_borrow_compiles_and_runs() {
    let source = r#"
func dup(s: &string) -> string {
    s.clone()
}
func main() -> i32 {
    val a: string = "hello"
    val b: string = dup(&a)
    return (a.len() as i32) + (b.len() as i32)
}
"#;
    // Both strings are 5 bytes → 10.
    run_expecting(source, 10);
}

#[test]
fn borrowing_a_struct_field_and_method() {
    let source = r#"
struct Point { x: i64, y: i64 }
impl Point {
    func sum(&self) -> i64 { self.x + self.y }
}
func read_x(p: &Point) -> i64 { p.x }
func read_sum(p: &Point) -> i64 { p.sum() }
func main() -> i32 {
    val pt = Point { x: 3, y: 4 }
    val x: i64 = read_x(&pt)
    val s: i64 = read_sum(&pt)
    return (x as i32) + (s as i32)
}
"#;
    // x = 3, sum = 7 → 10.
    run_expecting(source, 10);
}

#[test]
fn borrowing_a_temporary_is_rejected() {
    let source = r#"
func main() -> i32 {
    val r = &5
    return 0
}
"#;
    let (success, stderr) = check_source(source);
    assert!(!success, "borrowing a literal should be rejected");
    assert!(
        stderr.contains("cannot borrow"),
        "expected a borrow-place diagnostic, got: {stderr}"
    );
}

#[test]
fn borrowing_a_const_is_rejected() {
    // A `const` is an inlined value, not an addressable place.
    let source = r#"
const LIMIT: i32 = 10
func main() -> i32 {
    val r = &LIMIT
    return 0
}
"#;
    let (success, stderr) = check_source(source);
    assert!(!success, "borrowing a const should be rejected");
    assert!(
        stderr.contains("cannot borrow"),
        "expected a borrow-place diagnostic, got: {stderr}"
    );
}
