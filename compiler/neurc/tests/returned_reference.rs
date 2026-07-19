// Returned-reference outlives / lifetime elision tests (Phase 1.7).
//
// A function or method whose return type is a reference may only return a
// reference that outlives the call. Under lifetime elision a single input
// reference lifetime is applied to the output, so returning one of the
// reference parameters (or a borrow of `&self`) is sound; returning a borrow of
// a function-local value — or of a by-value parameter — would dangle and is
// rejected at compile time. Covers both end-to-end accept+run and the rejection
// diagnostics emitted by `neurc compile`.
mod common;
use common::CompileTest;

fn expect_compile_error(source: &str, needle: &str) {
    let test = CompileTest::new();
    let path = test.write_source("test.nr", source);
    let result = test.compile(&path);
    assert!(
        result.is_err(),
        "expected a returned-reference rejection, but compilation succeeded:\n{source}"
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
fn returning_a_borrow_of_a_local_is_rejected() {
    expect_compile_error(
        r#"
func dangle() -> &i32 {
    val local: i32 = 5
    return &local
}

func main() -> i32 {
    return 0
}
"#,
        "cannot return a reference to 'local'",
    );
}

#[test]
fn returning_a_borrow_of_a_by_value_parameter_is_rejected() {
    expect_compile_error(
        r#"
func dangle(n: i32) -> &i32 {
    return &n
}

func main() -> i32 {
    return 0
}
"#,
        "cannot return a reference to 'n'",
    );
}

#[test]
fn returning_through_a_local_reference_binding_is_rejected() {
    expect_compile_error(
        r#"
func leak() -> &i32 {
    val local: i32 = 7
    val r: &i32 = &local
    r
}

func main() -> i32 {
    return 0
}
"#,
        "cannot return a reference to 'local'",
    );
}

#[test]
fn returning_a_reference_parameter_compiles_and_runs() {
    run_expecting(
        r#"
func identity(r: &i32) -> &i32 {
    r
}

func main() -> i32 {
    val x: i32 = 42
    val same: &i32 = identity(&x)
    return *same
}
"#,
        42,
    );
}

#[test]
fn returning_one_of_two_reference_parameters_compiles_and_runs() {
    run_expecting(
        r#"
func first(a: &i32, b: &i32) -> &i32 {
    a
}

func main() -> i32 {
    val x: i32 = 17
    val y: i32 = 99
    val r: &i32 = first(&x, &y)
    return *r
}
"#,
        17,
    );
}
