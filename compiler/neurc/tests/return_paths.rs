// A non-void function must produce a value on every path.
//
// The checker recognised only a trailing bare expression as the implicit return, so a
// body that ended in anything else — a trailing `if` (which the parser always shapes as
// `Stmt::If`), a loop, a `val`, nothing at all — was never checked against the declared
// return type and never checked for producing a value. The backend then left the exit
// block without a return, LLVM terminated it with `unreachable` (a legal terminator, so
// the verifier stayed silent), and the program ran off the end of the function.

mod common;
use common::CompileTest;

/// Compile `source` and return the compiler's output, asserting it was rejected.
fn expect_rejected(filename: &str, source: &str) -> String {
    let test = CompileTest::new();
    let path = test.write_source(filename, source);
    match test.compile(&path) {
        Ok(_) => panic!("expected {} to be rejected, but it compiled", filename),
        Err(output) => output,
    }
}

#[test]
fn regression_empty_body_of_non_void_function_is_rejected() {
    let out = expect_rejected(
        "empty_body.nr",
        r#"
func f() -> i32 { }

func main() -> i32 { f() }
"#,
    );
    assert!(
        out.contains("missing return statement"),
        "expected a missing-return diagnostic, got: {out}"
    );
}

#[test]
fn regression_if_without_else_falls_off_the_end() {
    // The then-branch returns; the path where the condition is false does not.
    let out = expect_rejected(
        "if_no_else_falls_through.nr",
        r#"
func f(n: i32) -> i32 {
    if n > 0 { return 1 }
}

func main() -> i32 { f(0 - 5) }
"#,
    );
    assert!(
        out.contains("missing return statement"),
        "expected a missing-return diagnostic, got: {out}"
    );
}

#[test]
fn regression_while_loop_body_is_not_a_return_path() {
    // A `while` may run zero times, so a `return` inside it guarantees nothing.
    let out = expect_rejected(
        "while_return.nr",
        r#"
func f(n: i32) -> i32 {
    while n > 0 { return 1 }
}

func main() -> i32 { f(0 - 5) }
"#,
    );
    assert!(
        out.contains("missing return statement"),
        "expected a missing-return diagnostic, got: {out}"
    );
}

#[test]
fn regression_trailing_binding_is_not_a_return() {
    let out = expect_rejected(
        "trailing_val.nr",
        r#"
func f() -> i32 {
    val x = 3
}

func main() -> i32 { f() }
"#,
    );
    assert!(
        out.contains("missing return statement"),
        "expected a missing-return diagnostic, got: {out}"
    );
}

#[test]
fn regression_missing_return_in_inherent_method() {
    let out = expect_rejected(
        "method_missing_return.nr",
        r#"
@derive(Copy)
struct C { v: i32 }

impl C {
    func get(&self) -> i32 { }
}

func main() -> i32 {
    val c = C { v: 3 }
    c.get()
}
"#,
    );
    assert!(
        out.contains("missing return statement"),
        "expected a missing-return diagnostic, got: {out}"
    );
}

#[test]
fn regression_missing_return_in_trait_default_method() {
    let out = expect_rejected(
        "trait_default_missing_return.nr",
        r#"
trait T {
    func go(&self) -> i32 { }
}

@derive(Copy)
struct D { v: i32 }

impl T for D { }

func main() -> i32 {
    val d = D { v: 1 }
    d.go()
}
"#,
    );
    assert!(
        out.contains("missing return statement"),
        "expected a missing-return diagnostic, got: {out}"
    );
}

#[test]
fn regression_tail_if_arm_type_is_checked_against_the_return_type() {
    // Previously unchecked, so the mismatch reached the LLVM verifier as an internal
    // "return type does not match operand type" failure instead of a diagnostic.
    let out = expect_rejected(
        "tail_if_wrong_arm_type.nr",
        r#"
func f(n: i32) -> i32 {
    if n > 0 { true } else { false }
}

func main() -> i32 { f(0 - 1) }
"#,
    );
    assert!(
        out.contains("mismatch"),
        "expected a type-mismatch diagnostic, got: {out}"
    );
}

#[test]
fn regression_tail_if_mixing_return_and_value_is_rejected() {
    // `if n > 0 { return 1 } else { 2 }` silently evaluated to 0 on the else path: the
    // arm's value was dropped because the tail `if` was never a value position.
    // Expression position already rejected the same shape, so the tail now agrees.
    let out = expect_rejected(
        "tail_if_mixed_arms.nr",
        r#"
func f(n: i32) -> i32 {
    if n > 0 { return 1 } else { 2 }
}

func main() -> i32 { f(0 - 1) }
"#,
    );
    assert!(
        out.contains("mismatch"),
        "expected a type-mismatch diagnostic, got: {out}"
    );
}

#[test]
fn tail_if_where_every_arm_returns_still_compiles() {
    // Both arms leave the function, so the `if` carries no value and is a statement.
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "tail_if_all_return.nr",
            r#"
func f(n: i32) -> i32 {
    if n > 0 { return 1 } else { return 2 }
}

func main() -> i32 { f(0 - 1) }
"#,
        )
        .expect("compile/run failed");
    assert_eq!(exit, 2);
}

#[test]
fn early_return_followed_by_a_tail_expression_still_compiles() {
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "early_return_then_tail.nr",
            r#"
func f(n: i32) -> i32 {
    if n > 0 { return 1 }
    7
}

func main() -> i32 { f(0 - 1) }
"#,
        )
        .expect("compile/run failed");
    assert_eq!(exit, 7);
}

#[test]
fn a_body_that_only_panics_still_compiles() {
    // `panic` diverges, so it satisfies a non-void return type.
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "body_only_panics.nr",
            r#"
func f() -> i32 {
    panic("no")
}

func main() -> i32 {
    if 1 > 2 { f() } else { 5 }
}
"#,
        )
        .expect("compile/run failed");
    assert_eq!(exit, 5);
}
