// String-slice tests (Phase 1.7)
// `&string` is a borrowed string slice. Equality (`==` / `!=`) compares the
// underlying UTF-8 bytes for any combination of owned `string` and `&string`,
// auto-dereferencing a borrowed operand. Reference-peeling is limited to string,
// so `i32 == &string` and `&i32 == i32` remain type errors.
mod common;
use common::CompileTest;

use std::fs;
use std::process::Command;
use tempfile::TempDir;

/// Path to the `neurc` binary Cargo built for this test run.
///
/// Cargo sets `CARGO_BIN_EXE_neurc` for integration tests in the `neurc`
/// package; it is absolute and already carries the platform executable
/// suffix. Do not derive it from `current_exe()` — that assumes the legacy
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
fn two_string_slices_compare_equal() {
    let source = r#"
func eq(a: &string, b: &string) -> bool {
    a == b
}
func main() -> i32 {
    val x: string = "hello"
    val y: string = "hello"
    if eq(&x, &y) { return 0 }
    return 1
}
"#;
    run_expecting(source, 0);
}

#[test]
fn two_string_slices_compare_unequal() {
    let source = r#"
func eq(a: &string, b: &string) -> bool {
    a == b
}
func main() -> i32 {
    val x: string = "hello"
    val y: string = "world"
    if eq(&x, &y) { return 1 }
    return 0
}
"#;
    run_expecting(source, 0);
}

#[test]
fn slice_not_equal_operator() {
    let source = r#"
func main() -> i32 {
    val x: string = "abc"
    val y: string = "abd"
    if (&x != &y) { return 0 }
    return 1
}
"#;
    run_expecting(source, 0);
}

#[test]
fn slice_compares_against_owned_string() {
    // Mixed: a `&string` slice against an owned `string` literal, both orders.
    let source = r#"
func matches(s: &string) -> bool {
    s == "Neuro"
}
func main() -> i32 {
    val lang: string = "Neuro"
    val a: bool = matches(&lang)
    val b: bool = ("Neuro" == &lang)
    if a && b { return 0 }
    return 1
}
"#;
    run_expecting(source, 0);
}

#[test]
fn borrowing_for_comparison_does_not_move() {
    // Comparing through borrows must leave both bindings usable afterward.
    let source = r#"
func main() -> i32 {
    val x: string = "hello"
    val y: string = "hello"
    val eq: bool = (&x == &y)
    return (x.len() as i32) + (y.len() as i32) - 10
}
"#;
    run_expecting(source, 0);
}

#[test]
fn comparing_string_slice_with_int_is_rejected() {
    let source = r#"
func main() -> i32 {
    val x: string = "hello"
    val n: i32 = 5
    val bad: bool = (&x == n)
    return 0
}
"#;
    let (success, stderr) = check_source(source);
    assert!(
        !success,
        "comparing &string with i32 must be a type error; got: {stderr}"
    );
}

#[test]
fn comparing_int_slice_with_int_is_rejected() {
    // Reference-peeling is limited to string: `&i32 == i32` still needs the deref
    // operator, which has not landed, so this stays a type error.
    let source = r#"
func main() -> i32 {
    val n: i32 = 5
    val bad: bool = (&n == n)
    return 0
}
"#;
    let (success, stderr) = check_source(source);
    assert!(
        !success,
        "comparing &i32 with i32 must remain a type error; got: {stderr}"
    );
}

// --- string.slice(range) ------------------------------------------

/// Compile + run, asserting the process did NOT exit cleanly (a panic aborts via
/// SIGABRT, so `status.code()` is `None`, surfaced as a non-zero/`-1` code).
fn run_expecting_abort(source: &str) {
    let test = CompileTest::new();
    let code = test
        .compile_and_run("test.nr", source)
        .expect("program should compile and run");
    assert_ne!(
        code, 0,
        "expected the slice to panic and abort, but it exited 0:\n{source}"
    );
}

#[test]
fn slice_extracts_a_substring() {
    let source = r#"
func main() -> i32 {
    val s = "hello, world"
    if s.slice(0..5) == "hello" { return 0 }
    return 1
}
"#;
    run_expecting(source, 0);
}

#[test]
fn inclusive_slice_includes_the_upper_bound() {
    let source = r#"
func main() -> i32 {
    val s = "hello, world"
    if s.slice(7..=11) == "world" { return 0 }
    return 1
}
"#;
    run_expecting(source, 0);
}

#[test]
fn empty_slice_is_the_empty_string() {
    let source = r#"
func main() -> i32 {
    val s = "abc"
    if s.slice(1..1) == "" { return 0 }
    return 1
}
"#;
    run_expecting(source, 0);
}

#[test]
fn slice_len_is_the_byte_span() {
    let source = r#"
func main() -> i32 {
    val s = "hello, world"
    return s.slice(0..5).len() as i32
}
"#;
    run_expecting(source, 5);
}

#[test]
fn slice_of_a_borrowed_string_argument() {
    // `.slice` auto-derefs a `&string` receiver and the result re-borrows it.
    let source = r#"
func first_two(s: &string) -> bool {
    s.slice(0..2) == "ne"
}
func main() -> i32 {
    val lang = "neuro"
    if first_two(&lang) { return 0 }
    return 1
}
"#;
    run_expecting(source, 0);
}

#[test]
fn out_of_bounds_slice_aborts() {
    let source = r#"
func main() -> i32 {
    val s = "hi"
    val bad = s.slice(0..9)
    return 0
}
"#;
    run_expecting_abort(source);
}

#[test]
fn reversed_range_aborts() {
    let source = r#"
func main() -> i32 {
    val s = "hello"
    val bad = s.slice(4..1)
    return 0
}
"#;
    run_expecting_abort(source);
}

#[test]
fn mid_codepoint_slice_aborts() {
    // "é" is two UTF-8 bytes (0xC3 0xA9); slicing 0..1 splits the code point.
    let source = "
func main() -> i32 {
    val s = \"\u{e9}\"
    val bad = s.slice(0..1)
    return 0
}
";
    run_expecting_abort(source);
}

#[test]
fn slice_requires_a_range_argument() {
    let source = r#"
func main() -> i32 {
    val s = "hello"
    val bad = s.slice(3)
    return 0
}
"#;
    let (success, stderr) = check_source(source);
    assert!(
        !success,
        "slice with a non-range argument must be a type error; got: {stderr}"
    );
}

#[test]
fn range_outside_slice_is_rejected() {
    let source = r#"
func main() -> i32 {
    val r = 0..5
    return 0
}
"#;
    let (success, stderr) = check_source(source);
    assert!(
        !success,
        "a range expression outside .slice() must be a type error; got: {stderr}"
    );
}

#[test]
fn for_range_loop_still_parses_and_runs() {
    // Regression: adding `..` as an expression operator must not break for-ranges.
    let source = r#"
func main() -> i32 {
    mut total: i32 = 0
    for i in 0..5 { total = total + i }
    for j in 1..=3 { total = total + j }
    return total
}
"#;
    run_expecting(source, 16);
}

#[test]
fn a_returned_slice_survives_a_later_call() {
    // The slice is computed in `head`'s frame; returning it must hand the caller a
    // value, not a pointer into a frame that has already been reclaimed. `clobber`
    // is there to reuse that stack region before the slice is read.
    let source = r#"
func head(s: &string) -> &string {
    s.slice(0..2)
}

func clobber(x: i32) -> i32 {
    val a = "aaaaaaaaaaaaaaaaaaaa"
    val b = a.slice(0..9)
    b.len() as i32 + x
}

func main() -> i32 {
    val right = "spectrum"
    val prefix = head(&right)
    val j = clobber(0)
    prefix.len() as i32 + (j - j)
}
"#;
    run_expecting(source, 2);
}

#[test]
fn a_returned_slice_keeps_its_bytes_not_just_its_length() {
    // Length alone would survive a stale pointer by luck; comparing the bytes proves
    // the pointer half of the fat pointer is still the borrowee's, not reclaimed stack.
    let source = r#"
func head(s: &string) -> &string {
    s.slice(0..4)
}

func clobber() -> i32 {
    val a = "zzzzzzzzzzzzzzzzzzzz"
    val b = a.slice(0..17)
    b.len() as i32
}

func main() -> i32 {
    val right = "spectrum"
    val prefix = head(&right)
    val junk = clobber()
    if prefix == "spec" { return 0 }
    return junk
}
"#;
    run_expecting(source, 0);
}

#[test]
fn a_slice_is_computed_afresh_on_every_loop_iteration() {
    // The slice slot was the one alloca left at the builder position rather than the
    // function entry, so a slice inside a loop grew the stack. With the slice returned
    // by value there is no slot at all.
    let source = r#"
func main() -> i32 {
    val text = "abcdefghij"
    mut total = 0
    mut i = 0
    while i < 100000 {
        val part = text.slice(0..3)
        total = total + (part.len() as i32)
        i = i + 1
    }
    total / 100000
}
"#;
    run_expecting(source, 3);
}

#[test]
fn a_mutable_string_borrow_still_reaches_the_referent() {
    // `&string` is the fat pointer by value; `&mut string` is *not*, because a store
    // through it has to land on the caller's binding. The two halves of that split
    // must not drift.
    let source = r#"
func replace(s: &mut string) {
    *s = "worlds"
}

func main() -> i32 {
    mut a = "hello"
    replace(&mut a)
    a.len() as i32
}
"#;
    run_expecting(source, 6);
}

#[test]
fn a_slice_of_a_slice_keeps_borrowing_the_original_bytes() {
    let source = r#"
func mid(s: &string) -> &string {
    s.slice(1..4)
}

func main() -> i32 {
    val a = "abcdefgh"
    val m = mid(&a)
    val n = m.slice(1..3)
    if n == "cd" { return 0 }
    return 1
}
"#;
    run_expecting(source, 0);
}
