// A newline ends a statement unless the line that just ended asks to continue.
//
// The expression parser skipped newlines before consulting the next token's
// precedence, so the FOLLOWING line decided instead: a line starting with `(` was
// eaten as a call's argument list, and one starting with `[` as an index. `val a = f()`
// followed by `(2 + 3)` therefore parsed as `f()(2 + 3)`. Where the previous line's
// value happened to be callable — a closure binding — the misparse type-checked and
// silently ran a different program.

mod common;
use common::CompileTest;

fn run(filename: &str, source: &str) -> i32 {
    let test = CompileTest::new();
    test.compile_and_run(filename, source)
        .expect("compile/run failed")
}

#[test]
fn regression_parenthesized_expression_starts_a_new_statement() {
    let exit = run(
        "paren_line.nr",
        r#"
func f() -> i32 { 1 }

func main() -> i32 {
    val a = f()
    (2 + 3) + a
}
"#,
    );
    assert_eq!(exit, 6);
}

#[test]
fn regression_array_literal_starts_a_new_statement() {
    let exit = run(
        "bracket_line.nr",
        r#"
func f() -> i32 { 1 }

func main() -> i32 {
    val a = f()
    [4, 5, 6][2] + a
}
"#,
    );
    assert_eq!(exit, 7);
}

#[test]
fn regression_a_closure_binding_is_not_called_by_the_next_line() {
    // The silent case: `add` is callable, so gluing the next line on produced a
    // well-typed program that computed something the source never asked for.
    let exit = run(
        "closure_line.nr",
        r#"
func main() -> i32 {
    val add = |x: i32| -> i32 { x + 100 }
    val a = add
    (2 + 3)
}
"#,
    );
    assert_eq!(exit, 5);
}

#[test]
fn a_line_ending_in_an_operator_still_continues() {
    let exit = run(
        "trailing_operator.nr",
        r#"
func main() -> i32 {
    val a = 2 +
        3
    a * 2
}
"#,
    );
    assert_eq!(exit, 10);
}

#[test]
fn a_call_argument_list_may_still_span_lines() {
    let exit = run(
        "multiline_args.nr",
        r#"
func add3(a: i32, b: i32, c: i32) -> i32 { a + b + c }

func main() -> i32 {
    add3(
        1,
        2,
        3
    )
}
"#,
    );
    assert_eq!(exit, 6);
}

#[test]
fn a_method_chain_may_still_continue_on_the_next_line() {
    // A leading `.` cannot start a statement, so it stays a continuation.
    let exit = run(
        "method_chain.nr",
        r#"
struct C { v: i32 }

impl C {
    func get(&self) -> i32 { self.v }
}

func main() -> i32 {
    val c = C { v: 7 }
    val x = c
        .get()
    x + 1
}
"#,
    );
    assert_eq!(exit, 8);
}
