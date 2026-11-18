// End-to-end tests for explicit lifetime annotations: the `'a` in
// `func longest<'a>(a: &'a string, b: &'a string) -> &'a string`. Lifetimes are a
// well-formedness surface only: they are validated against the declared parameter
// list, then erased, so they add zero runtime cost and never change a reference's
// type. These tests exercise the full pipeline: parse → type-check → HIR lowering →
// LLVM codegen → native run.
use crate::compile_harness::CompileTest;

#[test]
fn longest_returns_the_longer_borrow() {
    // The canonical example. Returning either borrowed parameter is permitted by
    // the elision-based outlives analysis; the explicit `<'a>` is the annotation form.
    let test = CompileTest::new();
    let source = r#"
func longest<'a>(a: &'a string, b: &'a string) -> &'a string {
    if a.len() > b.len() { a } else { b }
}
func main() -> i32 {
    val x = "hello"
    val y = "hi there!"
    val r = longest(&x, &y)
    return r.len() as i32
}
"#;
    let exit = test
        .compile_and_run("lifetime_longest.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 9);
}

#[test]
fn lifetime_annotation_is_erased_at_call_site() {
    // `&'a string` is the same type as `&string`: an unannotated borrow satisfies a
    // parameter typed with an explicit lifetime.
    let test = CompileTest::new();
    let source = r#"
func take<'a>(s: &'a string) -> i32 {
    s.len() as i32
}
func main() -> i32 {
    val msg = "abcd"
    return take(&msg)
}
"#;
    let exit = test
        .compile_and_run("lifetime_erased.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 4);
}

#[test]
fn lifetime_alongside_type_parameter() {
    // A signature mixing a lifetime and a type parameter monomorphizes on the type
    // parameter only: the lifetime does not participate.
    let test = CompileTest::new();
    let source = r#"
func first_len<'a, T>(s: &'a string, _x: T) -> i32 {
    s.len() as i32
}
func main() -> i32 {
    val msg = "roger"
    return first_len(&msg, 7)
}
"#;
    let exit = test
        .compile_and_run("lifetime_with_type_param.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 5);
}

#[test]
fn undeclared_lifetime_fails_to_compile() {
    // `'b` is used but never declared: a well-formedness error rejected before codegen.
    let test = CompileTest::new();
    let source = r#"
func f<'a>(a: &'b string) -> i32 { 0 }
func main() -> i32 { return 0 }
"#;
    let path = test.write_source("lifetime_undeclared.nr", source);
    let err = test
        .compile(&path)
        .expect_err("an undeclared lifetime must be a compile error");
    assert!(err.contains("undeclared lifetime `'b`"), "{err}");
}

#[test]
fn regression_bug_099_a_returned_aggregate_may_not_borrow_a_local() {
    // A borrow carried out inside a struct, an enum payload, a tuple or an `Option`
    // dangles exactly as a bare `&s` does, and so does one in each arm of a tail `if`.
    let test = CompileTest::new();
    let err = test
        .check(
            "aggregate_borrow.nr",
            r#"
struct H { r: &string }
enum E { A(&string), B }
struct Outer { h: H, n: i32 }
func mk_struct() -> H {
    val s = "he" + "llo"
    H { r: &s }
}
func mk_enum() -> E {
    val s = "he" + "llo"
    E::A(&s)
}
func mk_tuple() -> (&string, i32) {
    val s = "he" + "llo"
    return (&s, 1)
}
func mk_nested(flag: bool) -> Outer {
    val s = "x" + "y"
    if flag { Outer { h: H { r: &s }, n: 1 } } else { Outer { h: H { r: &s }, n: 2 } }
}
func mk_option() -> Option<&string> {
    val s = "x" + "y"
    Option::Some(&s)
}
func main() -> i32 { 0 }
"#,
        )
        .expect_err("borrows of locals must not be returned");
    assert_eq!(
        err.matches("cannot return a reference to 's'").count(),
        6,
        "{err}"
    );
    let err = test
        .check(
            "if_tail_borrow.nr",
            r#"
func f(c: bool) -> &string {
    val s = "a" + "b"
    if c { &s } else { &s }
}
func main() -> i32 { 0 }
"#,
        )
        .expect_err("borrows of locals must not be returned");
    assert_eq!(
        err.matches("cannot return a reference to 's'").count(),
        2,
        "{err}"
    );
}

#[test]
fn regression_bug_099_a_returned_aggregate_may_borrow_a_parameter() {
    let exit = CompileTest::new()
        .compile_and_run(
            "aggregate_param_borrow.nr",
            r#"
struct H { r: &string }
enum E { A(&string), B }
func mk(s: &string) -> H { H { r: s } }
func mk2(s: &string) -> E { E::A(s) }
func mk3(s: &string) -> (&string, i32) { (s, 4) }
func main() -> i32 {
    val t = "abc"
    val h = mk(&t)
    val n = match mk2(&t) { E::A(r) => r.len(), E::B => 0 }
    val p = mk3(&t)
    (h.r.len() + n) as i32 + p.1 - 1
}
"#,
        )
        .expect("should compile and run");
    assert_eq!(exit, 9);
}
