//! Function values that move and are lent: an `Fn` value copies, an `FnMut` or `FnOnce`
//! value moves, and a value passed to a function-typed parameter is lent for the call,
//! shared for a plain parameter and exclusive for a `mut` one, so the caller keeps it.

use crate::compile_harness::CompileTest;

fn run(filename: &str, source: &str) -> i32 {
    CompileTest::new()
        .compile_and_run(filename, source)
        .expect("compile/run failed")
}

fn refused(filename: &str, source: &str) -> String {
    CompileTest::new()
        .check(filename, source)
        .expect_err("the program should be refused")
}

#[test]
fn a_mut_value_lent_to_each_call_keeps_every_write() {
    let source = r#"
func apply(f: mut (i32) -> void, x: i32) { f(x) }
func main() -> i32 {
    mut n = 0
    {
        val f = |x: i32| -> void { n += x }
        apply(f, 1)
        f(2)
        apply(f, 3)
    }
    n
}
"#;
    assert_eq!(run("lent_mut.nr", source), 6);
}

#[test]
fn a_mut_value_moves_with_its_owned_environment() {
    let source = r#"
struct Ticker { tick: mut () -> u64 }
func counter(word: string) -> mut () -> u64 {
    mut total: u64 = 0
    move || -> u64 {
        total += word.len()
        total
    }
}
func twice(f: mut () -> u64) -> u64 { f() + f() }
func finish(f: once () -> u64) -> u64 { f() }
func main() -> i32 {
    val c = counter("ab" + "cd")
    val a = twice(c)
    val moved = c
    val held = Ticker { tick: moved }
    val t = held.tick
    val doubled = move || -> u64 { t() * 2 }
    val b = twice(doubled)
    val last = finish(counter("xyz" + "w"))
    (a + b + last) as i32
}
"#;
    // a = 4 + 8, b = 2 * 12 + 2 * 16, last = 4
    assert_eq!(run("moved_mut.nr", source), 12 + 56 + 4);
}

#[test]
fn an_argument_call_returns_before_the_call_it_feeds() {
    let source = r#"
func apply(f: mut (i32) -> i32, x: i32) -> i32 { f(x) }
func main() -> i32 {
    mut n = 0
    mut r = 0
    {
        val f = |x: i32| -> i32 {
            n += x
            n
        }
        r = f(f(1))
        r += apply(f, f(2))
        r += f(1) + f(1)
    }
    r + n
}
"#;
    assert_eq!(run("two_phase.nr", source), 29 + 10);
}

#[test]
fn a_shared_value_still_copies() {
    let source = r#"
func make(k: i32) -> (i32) -> i32 { |x| x + k }
func two(a: (i32) -> i32, b: (i32) -> i32) -> i32 { a(1) + b(2) }
func main() -> i32 {
    val f = make(10)
    val g = f
    two(f, g) + two(f, f) + g(3)
}
"#;
    assert_eq!(run("shared_copy.nr", source), 23 + 23 + 13);
}

#[test]
fn a_moved_mut_value_cannot_be_called() {
    let err = refused(
        "moved_mut.nr",
        r#"
func main() -> i32 {
    mut n = 0
    {
        val f = |x: i32| -> void { n += x }
        val g = f
        g(1)
        f(2)
    }
    n
}
"#,
    );
    assert!(err.contains("use of moved value 'f'"), "{err}");
}

#[test]
fn a_mut_value_cannot_reach_its_own_running_call() {
    for body in [
        "two(f, f)",
        "two(f, |g: mut () -> void| -> void { f(g) })",
        "f(|| -> void { f(|| -> void {}) })",
    ] {
        let err = refused(
            "reentrant.nr",
            &format!(
                r#"
func two(a: mut (mut () -> void) -> void, b: mut (mut () -> void) -> void) {{
    a(|| -> void {{}})
    b(|| -> void {{}})
}}
func main() -> i32 {{
    mut n = 0
    {{
        val f = |g: mut () -> void| -> void {{
            n += 1
            g()
        }}
        {body}
    }}
    n
}}
"#
            ),
        );
        assert!(
            err.contains("cannot borrow 'f' as mutable"),
            "{body}: {err}"
        );
        assert!(!err.contains("type mismatch"), "{body}: {err}");
    }
}

#[test]
fn a_closure_calling_a_mut_value_holds_it_exclusively() {
    let err = refused(
        "held.nr",
        r#"
func main() -> i32 {
    mut n = 0
    {
        val f = |x: i32| -> void { n += x }
        val h = |x: i32| -> void { f(x) }
        h(1)
        f(2)
        h(3)
    }
    n
}
"#,
    );
    assert!(err.contains("cannot borrow 'f' as mutable"), "{err}");
}

#[test]
fn a_mut_value_behind_a_shared_borrow_is_not_called() {
    let err = refused(
        "shared_field.nr",
        r#"
struct Holder { step: mut (i32) -> void }
impl Holder {
    func run(&self) { (self.step)(1) }
}
func main() -> i32 { 0 }
"#,
    );
    assert!(err.contains("through a shared `&` borrow"), "{err}");
}
