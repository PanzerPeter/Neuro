// Borrow exclusivity tests (Phase 1.7)
//
// Verifies the flow-sensitive aliasing rules enforced by the borrow checker:
// at most one `&mut` borrow of a place may be live at a time, and no `&` borrow
// may coexist with a live `&mut`. A borrow held by a `val`/`mut` binding lives
// until that binding leaves scope; a borrow passed to a call or used inline ends
// with the statement that took it. Covers both end-to-end accept+run and the
// rejection diagnostics emitted by `neurc compile`.
mod common;
use common::CompileTest;

fn expect_compile_error(source: &str, needle: &str) {
    let test = CompileTest::new();
    let path = test.write_source("test.nr", source);
    let result = test.compile(&path);
    assert!(
        result.is_err(),
        "expected a borrow-checker rejection, but compilation succeeded:\n{source}"
    );
    let message = result.expect_err("just asserted is_err");
    assert!(
        message.contains(needle),
        "expected diagnostic to contain {needle:?}, got:\n{message}"
    );
}

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
fn two_live_mutable_borrows_are_rejected() {
    expect_compile_error(
        r#"
func main() -> i32 {
    mut x: i32 = 5
    val a: &mut i32 = &mut x
    val b: &mut i32 = &mut x
    *a = 1
    *b = 2
    return 0
}
"#,
        "as mutable",
    );
}

#[test]
fn mutable_borrow_while_shared_is_live_is_rejected() {
    expect_compile_error(
        r#"
func main() -> i32 {
    mut x: i32 = 5
    val a: &i32 = &x
    val b: &mut i32 = &mut x
    *b = 1
    return 0
}
"#,
        "as mutable",
    );
}

#[test]
fn shared_borrow_while_mutable_is_live_is_rejected() {
    expect_compile_error(
        r#"
func main() -> i32 {
    mut x: i32 = 5
    val a: &mut i32 = &mut x
    val b: &i32 = &x
    *a = 1
    return 0
}
"#,
        "as immutable",
    );
}

#[test]
fn sequential_transient_mutable_borrows_run() {
    // Each `&mut x` passed to `inc` ends with that call, so a third, longer-lived
    // `&mut x` is free to take its own exclusive borrow afterwards.
    run_expecting(
        r#"
func inc(n: &mut i32) { *n = *n + 1 }
func main() -> i32 {
    mut x: i32 = 40
    inc(&mut x)
    inc(&mut x)
    val r: &mut i32 = &mut x
    *r = *r + 3
    return *r
}
"#,
        45,
    );
}

#[test]
fn borrow_released_at_scope_exit_runs() {
    // The branch-scoped `&mut x` is released when the `if` body ends, leaving `x`
    // free for the later exclusive borrow `b`.
    run_expecting(
        r#"
func main() -> i32 {
    mut x: i32 = 10
    if true {
        val a: &mut i32 = &mut x
        *a = 20
    }
    val b: &mut i32 = &mut x
    *b = *b + 5
    return x
}
"#,
        25,
    );
}
