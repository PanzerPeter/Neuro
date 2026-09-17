// Borrow exclusivity tests (Phase 1.7)
//
// Verifies the flow-sensitive aliasing rules enforced by the borrow checker:
// at most one `&mut` borrow of a place may be live at a time, and no `&` borrow
// may coexist with a live `&mut`. A borrow held by a `val`/`mut` binding lives
// until that binding leaves scope; a borrow passed to a call or used inline ends
// with the statement that took it. Covers both end-to-end accept+run and the
// rejection diagnostics emitted by `neurc compile`.
use crate::compile_harness::CompileTest;

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
    // free for the later exclusive borrow `b`. `b` is still live at the return, so
    // the value is read back through the borrow rather than through `x`'s own name.
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
    return *b
}
"#,
        25,
    );
}

#[test]
fn reading_the_borrowee_while_mutably_borrowed_is_rejected() {
    expect_compile_error(
        r#"
func main() -> i32 {
    mut n: i32 = 1
    val r: &mut i32 = &mut n
    val read: i32 = n
    *r = 5
    return read
}
"#,
        "cannot use 'n' while it is mutably borrowed",
    );
}

#[test]
fn moving_the_borrowee_out_from_under_a_borrow_is_rejected() {
    // The unsound half: `b` would be left pointing into a buffer `s` gave away.
    expect_compile_error(
        r#"
func consume(s: string) -> u64 { s.len() }
func main() -> i32 {
    val s: string = "hello"
    val b: &string = &s
    val n: u64 = consume(s)
    return n as i32
}
"#,
        "cannot move out of 's' while it is borrowed",
    );
}

#[test]
fn assigning_to_the_borrowee_is_rejected() {
    expect_compile_error(
        r#"
func main() -> i32 {
    mut n: i32 = 1
    val r: &i32 = &n
    n = 5
    return *r
}
"#,
        "cannot assign to 'n' while it is borrowed",
    );
}

#[test]
fn reads_through_a_live_shared_borrow_run() {
    // A shared borrow leaves the borrowee readable through its own name: only the
    // exclusive borrow freezes it.
    run_expecting(
        r#"
func main() -> i32 {
    val x: i32 = 21
    val a: &i32 = &x
    val b: &i32 = &x
    return x + *a + *b - 21
}
"#,
        42,
    );
}

#[test]
fn a_transient_mutable_borrow_frees_the_name_when_its_call_returns() {
    // `bump(&mut n)` is finished before the second operand is evaluated, so reading
    // `n` there is not a read under a live borrow even though the statement goes on.
    run_expecting(
        r#"
func bump(n: &mut i32) -> i32 {
    *n = *n + 1
    return *n
}
func combine(a: i32, b: i32) -> i32 { a + b }
func main() -> i32 {
    mut n: i32 = 20
    return combine(bump(&mut n), n)
}
"#,
        42,
    );
}
