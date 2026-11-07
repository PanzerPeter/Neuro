// Returned-reference outlives / lifetime elision tests (Phase 1.7).
//
// A function or method whose return type is a reference may only return a
// reference that outlives the call. Under lifetime elision a single input
// reference lifetime is applied to the output, so returning one of the
// reference parameters (or a borrow of `&self`) is sound; returning a borrow of
// a function-local value (or of a by-value parameter) would dangle and is
// rejected at compile time. Covers both end-to-end accept+run and the rejection
// diagnostics emitted by `neurc compile`.
use crate::compile_harness::CompileTest;

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

/// An elided lifetime in a method's result borrows the receiver, so a method call on an
/// owned local cannot be returned. A result naming the impl's lifetime returns what the
/// receiver borrows, which outlives it.
#[test]
fn regression_bug_128_a_method_reference_into_a_local_receiver_is_refused() {
    expect_compile_error(
        r#"
struct B { s: string }
impl B {
    func head(&self) -> &string { self.s.slice(0..1) }
}
func first() -> &string {
    val b = B { s: "x" + "y" }
    b.head()
}
func main() -> i32 { first().len() as i32 }
"#,
        "cannot return a reference to 'b'",
    );
    let test = CompileTest::new();
    let source = r#"
struct Src<'a> { s: &'a string }
struct View<'a> { s: &'a string }
struct B { s: string }

impl<'a> Src<'a> {
    func source(&self) -> &'a string { self.s }
    func view(&self) -> View<'a> { View { s: self.s } }
}

impl B {
    func head(&self) -> &string { self.s.slice(0..1) }
}

func pick<'a>(s: &'a string) -> &'a string {
    val w = Src { s: s }
    w.source()
}

func view_of<'a>(s: &'a string) -> View<'a> {
    val w = Src { s: s }
    w.view()
}

func through(b: &B) -> &string { b.head() }

func main() -> i32 {
    val t = "abc" + "d"
    val b = B { s: "xy" + "z" }
    pick(&t).len() as i32 + view_of(&t).s.len() as i32 + through(&b).len() as i32
}
"#;
    assert_eq!(test.compile_and_run("lifetime_results.nr", source), Ok(9));
}

/// A receiver built for the call dies with its statement, so a result borrowing it cannot
/// be bound or returned, though it can be read in place.
#[test]
fn a_result_borrowing_a_temporary_receiver_is_not_bound() {
    let source = |body: &str| {
        format!(
            r#"
struct M {{ v: Vec<i32> }}
impl M {{ func view(&self) -> &Vec<i32> {{ &self.v }} }}
func make() -> M {{ M {{ v: Vec::new() }} }}
{body}
"#
        )
    };
    expect_compile_error(
        &source("func main() -> i32 {\n    val r = make().view()\n    r.len() as i32\n}"),
        "may outlive its statement",
    );
    for escape in [
        "func leak() -> &Vec<i32> { make().view() }\nfunc main() -> i32 { 0 }",
        "struct H<'a> { r: &'a Vec<i32> }\nfunc main() -> i32 {\n    val h = H { r: make().view() }\n    h.r.len() as i32\n}",
        "func main() -> i32 {\n    val keep = make()\n    mut r = keep.view()\n    r = make().view()\n    r.len() as i32\n}",
        "func same(v: &Vec<i32>) -> &Vec<i32> { v }\nfunc main() -> i32 {\n    val r = same(make().view())\n    r.len() as i32\n}",
    ] {
        expect_compile_error(&source(escape), "may outlive its statement");
    }
}
