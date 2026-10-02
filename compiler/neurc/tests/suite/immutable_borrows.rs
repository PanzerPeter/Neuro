// Immutable borrow tests (Phase 1.7)
// Verifies `&T` reference types and `&place` borrow expressions: borrowing does
// not move the borrowee, references are Copy, and method/field access auto-derefs
// through a borrow. Covers end-to-end compile+run and the borrow-a-temporary error.
use crate::compile_harness::CompileTest;

use std::fs;
use std::process::Command;
use tempfile::TempDir;

/// Path to the `neurc` binary Cargo built for this test run.
///
/// Cargo sets `CARGO_BIN_EXE_neurc` for integration tests in the `neurc`
/// package; it is absolute and already carries the platform executable
/// suffix. Do not derive it from `current_exe()`. That assumes the legacy
/// `target/<profile>/deps/` layout and breaks under Cargo's build-dir layout.
fn neurc_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_neurc"))
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

#[test]
fn regression_a_reference_returned_by_a_call_keeps_its_source_borrowed() {
    // `id(&s)` hands the borrow back, so `b` holds `s` exactly as `&s` would. Moving `s`
    // into `consume` used to compile and let `b.len()` read the freed buffer.
    let source = r#"
func id(s: &string) -> &string { s }
func consume(s: string) -> u64 { s.len() }

func main() -> i32 {
    val s: string = "hello"
    val b: &string = id(&s)
    val n: u64 = consume(s)
    return b.len() as i32
}
"#;
    let (success, stderr) = check_source(source);
    assert!(
        !success,
        "moving the source of a live returned reference must be refused"
    );
    assert!(
        stderr.contains("cannot move out of 's' while it is borrowed"),
        "expected the borrowee diagnostic, got: {stderr}"
    );
}

/// BUG-093: when an inner block's shadow died, the borrow it held was released against
/// the outer binding of the same name, which then could be reassigned under a live view.
#[test]
fn regression_bug_093_dying_shadow_keeps_the_outer_borrow() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut v: Vec<i32> = Vec::new()
    v.push(7)
    v.push(8)
    val s = v.slice(0..2)
    {
        val v = 3
        val r = &v
    }
    v = Vec::new()
    s[1]
}
"#;
    let err = test
        .check("shadow_release.nr", source)
        .expect_err("`v` is still borrowed by `s`");
    assert!(
        err.contains("cannot assign to 'v' while it is borrowed"),
        "got: {err}"
    );
}

/// BUG-095: a borrow handed out through a block, `if` or `match` tail ended with the block,
/// so the binding holding it did not freeze its borrowee. Written directly, the same
/// program was refused. The tail may reach the borrow through a block binding, a
/// destructured part, or a call returning one of its borrowed arguments.
#[test]
fn regression_bug_095_borrow_through_a_tail_is_held() {
    let test = CompileTest::new();
    for (name, init) in [
        (
            "tail_block.nr",
            "{\n        val q = v.slice(0..2)\n        q\n    }",
        ),
        (
            "tail_if.nr",
            "if c { v.slice(0..2) } else { v.slice(0..1) }",
        ),
        ("tail_ref.nr", "{\n        val v = &v\n        v\n    }"),
        (
            "tail_destructure.nr",
            "{\n        val (a, b) = (v.slice(0..1), 1)\n        a\n    }",
        ),
        (
            "tail_call.nr",
            "{\n        val q = pick(&v)\n        q\n    }",
        ),
    ] {
        let source = format!(
            r#"
func pick(s: &Vec<i32>) -> &Vec<i32> {{ s }}
func main() -> i32 {{
    mut v: Vec<i32> = Vec::new()
    v.push(7)
    val c = true
    val r = {init}
    v = Vec::new()
    r.len() as i32
}}
"#
        );
        let err = test
            .check(name, &source)
            .expect_err("`v` is still borrowed by `r`");
        assert!(
            err.contains("cannot assign to 'v' while it is borrowed"),
            "{name}: {err}"
        );
    }
}
