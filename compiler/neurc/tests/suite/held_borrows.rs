// End-to-end tests for borrows held inside values: a struct field, an enum payload,
// `Option<&T>`. A struct or enum may declare lifetime parameters for them, and the
// binding that holds such a value holds its borrows: the borrowee stays frozen while
// it lives, the holder may not outlive the borrowee, and it may not leave the
// function carrying a borrow of a local.
use crate::compile_harness::CompileTest;

#[test]
fn a_struct_with_a_lifetime_parameter_runs() {
    let test = CompileTest::new();
    let source = r#"
struct View<'a> { text: &'a string, from: u64 }

impl<'a> View<'a> {
    func rest(&self) -> u64 { self.text.len() - self.from }
    func source(&self) -> &'a string { self.text }
}

func view<'a>(text: &'a string, from: u64) -> View<'a> {
    View { text: text, from: from }
}

func main() -> i32 {
    val line = "neuro compiler"
    val v = view(&line, 6)
    val again: View<'_> = View { text: v.source(), from: 2 }
    (v.rest() + again.rest()) as i32
}
"#;
    let exit = test
        .compile_and_run("held_struct_view.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 8 + 12);
}

#[test]
fn an_enum_payload_and_option_hold_references() {
    let test = CompileTest::new();
    let source = r#"
enum Pick<'a, T> { Borrowed(&'a T), Owned(T) }

func value(p: &Pick<'_, i32>) -> i32 {
    match p {
        Pick::Borrowed(r) => *r,
        Pick::Owned(v) => v,
    }
}

func longer<'a>(a: &'a string, b: &'a string) -> Option<&'a string> {
    if a.len() == b.len() { None } else if a.len() > b.len() { Some(a) } else { Some(b) }
}

func main() -> i32 {
    val n = 30
    val held = Pick::Borrowed(&n)
    val own: Pick<i32> = Pick::Owned(4)
    val x = "short"
    val y = "much longer"
    val tie = "tie!"
    val best = longer(&x, &y)
    val none = longer(&tie, &tie)
    val len = match best { Some(s) => s.len() as i32, None => 0 }
    val fallback = (none ?? &x).len() as i32
    val mapped = best.map(|s| s.len()).ok_or(0u64) ?? 0
    value(&held) + value(&own) + len + fallback + mapped as i32
}
"#;
    let exit = test
        .compile_and_run("held_enum_option.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 30 + 4 + 11 + 5 + 11);
}

#[test]
fn a_held_mutable_borrow_writes_through_and_releases_with_its_scope() {
    let test = CompileTest::new();
    let source = r#"
struct Counter<'a> { hits: &'a mut i32 }

impl<'a> Counter<'a> {
    func bump(&mut self, by: i32) { *self.hits = *self.hits + by }
}

func main() -> i32 {
    mut total = 1
    {
        mut c = Counter { hits: &mut total }
        c.bump(10)
        c.bump(20)
    }
    total = total + 1
    total
}
"#;
    let exit = test
        .compile_and_run("held_counter.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 32);
}

fn refused(test: &CompileTest, name: &str, source: &str, message: &str) {
    let err = test
        .check(name, source)
        .expect_err("the program must be refused");
    assert!(err.contains(message), "expected `{message}` in:\n{err}");
}

#[test]
fn a_held_borrowee_cannot_be_moved_or_replaced() {
    let test = CompileTest::new();
    refused(
        &test,
        "held_move.nr",
        r#"
struct H { r: &string }
func main() -> i32 {
    val s = "hi"
    val h = H { r: &s }
    val t = s
    h.r.len() as i32
}
"#,
        "cannot move out of 's' while it is borrowed",
    );
    refused(
        &test,
        "held_assign.nr",
        r#"
func main() -> i32 {
    mut s = "hi"
    val o = Some(&s)
    s = "bye"
    0
}
"#,
        "cannot assign to 's' while it is borrowed",
    );
}

#[test]
fn test_bug_099_a_struct_returned_through_a_binding_is_refused() {
    let test = CompileTest::new();
    refused(
        &test,
        "bug_099.nr",
        r#"
struct H { r: &string }

func make() -> H {
    val s = "he" + "llo"
    val h = H { r: &s }
    h
}

func main() -> i32 {
    val h = make()
    h.r.len() as i32
}
"#,
        "cannot return a reference to 's'",
    );
}

#[test]
fn test_bug_109_a_holder_cannot_outlive_its_borrowee() {
    let test = CompileTest::new();
    refused(
        &test,
        "bug_109.nr",
        r#"
func main() -> i32 {
    val t: Vec<i32> = Vec::new()
    mut b: &Vec<i32> = &t
    {
        mut v: Vec<i32> = Vec::new()
        v.push(41)
        b = &v
    }
    return b[0]
}
"#,
        "'v' does not live long enough",
    );
    refused(
        &test,
        "held_outlives_call.nr",
        r#"
struct H { r: &string }
func set(h: &mut H, s: &string) { h.r = s }
func main() -> i32 {
    val t = "x"
    mut h = H { r: &t }
    {
        val s = "a" + "b"
        set(&mut h, &s)
    }
    h.r.len() as i32
}
"#,
        "'s' does not live long enough",
    );
    // Declared beside the borrowee, the same store compiles and runs.
    let exit = test
        .compile_and_run(
            "held_same_scope.nr",
            r#"
struct H { r: &string }
func set(h: &mut H, s: &string) { h.r = s }
func main() -> i32 {
    val t = "x"
    val s = "a" + "b"
    mut h = H { r: &t }
    set(&mut h, &s)
    h.r.len() as i32
}
"#,
        )
        .expect("compile/run failed");
    assert_eq!(exit, 2);
}

#[test]
fn a_local_borrow_cannot_be_stored_through_a_parameter() {
    let test = CompileTest::new();
    refused(
        &test,
        "held_store_param.nr",
        r#"
struct H { r: &string }
func fill(h: &mut H) {
    val s = "a" + "b"
    h.r = &s
}
func main() -> i32 { 0 }
"#,
        "cannot store a borrow of 's' through a reference",
    );
}

#[test]
fn an_undeclared_lifetime_argument_is_refused() {
    let test = CompileTest::new();
    refused(
        &test,
        "held_undeclared.nr",
        r#"
struct H<'a> { r: &'a string }
func len(h: H<'b>) -> i32 { h.r.len() as i32 }
func main() -> i32 { 0 }
"#,
        "undeclared lifetime `'b`",
    );
}
