// End-to-end tests for explicit lifetime annotations: the `'a` in
// `func longest<'a>(a: &'a string, b: &'a string) -> &'a string`. Lifetimes are a
// well-formedness surface only — they are validated against the declared parameter
// list, then erased, so they add zero runtime cost and never change a reference's
// type. These tests exercise the full pipeline: parse → type-check → HIR lowering →
// LLVM codegen → native run.
mod common;
use common::CompileTest;

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
    // parameter only — the lifetime does not participate.
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
    // `'b` is used but never declared — a well-formedness error rejected before codegen.
    let test = CompileTest::new();
    let source = r#"
func f<'a>(a: &'b string) -> i32 { 0 }
func main() -> i32 { return 0 }
"#;
    let path = test.write_source("lifetime_undeclared.nr", source);
    assert!(
        test.compile(&path).is_err(),
        "an undeclared lifetime must be a compile error"
    );
}
