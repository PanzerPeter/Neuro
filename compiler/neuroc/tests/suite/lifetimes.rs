// End-to-end tests for explicit lifetime annotations: the `'a` in
// `func longest<'a>(a: &'a string, b: &'a string) -> &'a string`. They are validated
// against the declared parameter list and erased, so they add zero runtime cost and never
// change a reference's type. What they do decide is what a call keeps: its result, or a
// `&mut` argument's referent, holds only the arguments whose types name one of its
// lifetimes, and the callee's body is held to that. These tests exercise the full
// pipeline: parse → type-check → HIR lowering → LLVM codegen → native run.
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

/// A use of a type names all of its lifetime arguments or none: `View<'a>`, `View<'_>` and
/// `View` are the same type, and so is a type alias given the lifetime. Counted, then erased.
#[test]
fn every_lifetime_argument_or_none_compiles() {
    let source = r#"
struct View<'a> { r: &'a i32 }
enum Pick<'a, T> { Borrowed(&'a T), Owned(T) }
type V = View
impl<'a> View<'a> {
    func get(&self) -> i32 { *self.r }
}
func named<'a>(v: View<'a>) -> i32 { v.get() }
func anonymous(v: View<'_>) -> i32 { v.get() }
func mixed<'a>(p: Pick<'a, i32>) -> i32 {
    match p {
        Pick::Borrowed(r) => *r,
        Pick::Owned(n) => n
    }
}
func aliased<'a>(v: V<'a>) -> i32 { v.get() }
func elided(v: View) -> i32 { v.get() }
func main() -> i32 {
    val x = 4
    val w: View<'_> = View { r: &x }
    return named(View { r: &x }) + anonymous(w) + mixed(Pick::Owned(1))
        + elided(View { r: &x }) + aliased(View { r: &x })
}
"#;
    assert_eq!(
        crate::compile_harness::run("lifetime_counts.nr", source),
        17
    );
}

/// A written lifetime-argument list is as long as the declaration's, wherever the type is named:
/// a parameter, an annotation, an `impl` header. A type that declares none takes none.
#[test]
fn a_wrong_lifetime_argument_count_is_refused() {
    let decls = "struct View<'a> { r: &'a i32 }\nenum Pick<'a, T> { Borrowed(&'a T), Owned(T) }\n";
    for (body, expected) in [
        (
            "func f<'a>(v: View<'a, 'a>) -> i32 { 0 }",
            "`View` declares 1 lifetime parameter(s), but 2 lifetime argument(s) are written",
        ),
        (
            "func f<'a, 'b>(p: Pick<'a, 'b, i32>) -> i32 { 0 }",
            "`Pick` declares 1 lifetime parameter(s), but 2",
        ),
        (
            "impl<'a> View<'a, 'a> {}",
            "`View` declares 1 lifetime parameter(s), but 2",
        ),
        (
            "func f<'a>(x: i32<'a>) -> i32 { x }",
            "`i32` declares no lifetime parameter, so it takes no lifetime argument",
        ),
        (
            "func f<'a, T>(t: T<'a>) -> i32 { 0 }",
            "`T` declares no lifetime parameter",
        ),
        (
            "func f<'a>(v: Vec<'a, i32>) -> i32 { 0 }",
            "`Vec` declares no lifetime parameter",
        ),
        (
            "func f() -> i32 {\n    val x = 1\n    val w: View<'_, '_> = View { r: &x }\n    0\n}",
            "`View` declares 1 lifetime parameter(s), but 2",
        ),
    ] {
        let source = format!("{decls}{body}\nfunc main() -> i32 {{ 0 }}\n");
        crate::compile_harness::assert_check_error("lifetime_count.nr", &source, expected);
    }
}

/// A result naming one argument's lifetime leaves the other argument free, a local
/// included, and a `&mut` argument takes only what its referent's lifetime names.
#[test]
fn regression_explicit_lifetimes_narrow_what_a_call_keeps() {
    let test = CompileTest::new();
    let source = r#"
struct Slot<'a> { text: &'a string }

func first<'a>(keep: &'a string, probe: &string) -> &'a string {
    if probe.len() > 0 { keep } else { keep }
}

func fill<'a>(slot: &mut Slot<'a>, text: &'a string, probe: &string) {
    if probe.len() > 1 { slot.text = text }
}

func pass_through<'a>(keep: &'a string) -> &'a string {
    val local = "scratch".clone()
    first(keep, &local)
}

func main() -> i32 {
    val outer = "outer text".clone()
    mut probe = "p".clone()
    val r = first(&outer, &probe)
    probe = "pq".clone()
    val longer = "longer text here".clone()
    mut slot = Slot { text: &outer }
    {
        val inner = "inner".clone()
        fill(&mut slot, &longer, &inner)
        fill(&mut slot, &outer, &probe)
    }
    (r.len() + slot.text.len() + pass_through(&outer).len()) as i32
}
"#;
    assert_eq!(
        test.run_leak_checked("lifetime_narrowing.nr", source),
        10 + 10 + 10
    );
}

/// BUG-172: a call storing a local's borrow into what a `&mut` parameter points at
/// compiled, and the caller then read the local's freed storage.
#[test]
fn regression_bug_172_a_local_stored_through_a_mut_parameter_by_a_call_is_refused() {
    let source = r#"
struct H<'a> { r: &'a string }
func set<'a>(h: &mut H<'a>, s: &'a string) { h.r = s }
func fill(h: &mut H) {
    val local = "dangling".clone()
    set(h, &local)
}
func main() -> i32 {
    val keep = "keep"
    mut h = H { r: &keep }
    fill(&mut h)
    h.r.len() as i32
}
"#;
    crate::compile_harness::assert_check_error(
        "bug_172.nr",
        source,
        "cannot store a borrow of 'local' through a reference",
    );
}
