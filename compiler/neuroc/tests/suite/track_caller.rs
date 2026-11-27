// End-to-end tests for `@track_caller`: a `panic`, `assert` or `unreachable` in the marked
// function's own body names the line of the call that reached it, through any chain of marked
// callers, at every optimization level.

use crate::compile_harness::{compile_tagged, rejected, run_expecting};
use std::process::Command;

const CHAIN: &str = r#"
@track_caller
func positive(x: i32) -> i32 {
    assert(x > 0)
    x
}
@track_caller
func checked(x: i32) -> i32 {
    if x > 100 {
        panic("too big: {x}")
    }
    if x == 50 {
        unreachable()
    }
    positive(x)
}
@track_caller
func widened(x: i32) -> i32 { checked(x) * 2 }
func inner_closure(n: i32) -> i32 {
    val f = |y: i32| -> i32 { positive(y) }
    f(n)
}
func main() -> i32 {
    val ok = widened(3) + inner_closure(4)
    val which = ok - 10
    if which == 0 { widened(WHICH) } else { 0 }
}
"#;

/// The diagnostic `CHAIN` aborts with when `main`'s failing call passes `which`.
fn diagnostic(which: &str, optimization: &str) -> String {
    let source = CHAIN.replace("WHICH", which);
    let exe = compile_tagged(
        &source,
        &format!("track_{which}_{optimization}"),
        optimization,
    );
    let output = Command::new(&exe).output().expect("run executable");
    assert_ne!(output.status.code(), Some(0), "the call should abort");
    String::from_utf8_lossy(&output.stderr).replace("\r\n", "\n")
}

#[test]
fn a_tracked_panic_names_the_outermost_untracked_call() {
    // `widened(WHICH)` is on line 26 of `CHAIN`, column 21.
    for optimization in ["0", "2"] {
        for (which, head) in [
            ("-1", "assertion failed at "),
            ("200", "panic: too big: 200 at "),
            ("50", "internal error: entered unreachable code at "),
        ] {
            let text = diagnostic(which, optimization);
            assert!(text.starts_with(head), "-O{optimization} {which}: {text}");
            assert!(
                text.ends_with(":26:21\n"),
                "-O{optimization} {which}: {text}"
            );
        }
    }
}

#[test]
fn a_tracked_function_runs_unchanged_and_a_closure_in_one_is_its_own() {
    // `widened(3) + inner_closure(4)` is 10, so `main` returns `widened(1)`. The closure
    // in `inner_closure` calls `positive` itself, from a body that is not marked.
    run_expecting(&CHAIN.replace("WHICH", "1"), 2);
}

#[test]
fn track_caller_is_refused_where_no_call_site_reaches_it() {
    for (source, problem) in [
        (
            "@track_caller\nfunc f(x: i32) -> i32 { x }\nfunc main() -> i32 {\n    val g = f\n    g(1)\n}\n",
            "is called by name; it is not a function value",
        ),
        (
            "struct S { a: i32 }\nimpl S {\n    @track_caller\n    func m(&self) -> i32 { self.a }\n}\nfunc main() -> i32 { 0 }\n",
            "marks a free function, not a method",
        ),
        (
            "@track_caller\n@test\nfunc t() { }\nfunc main() -> i32 { 0 }\n",
            "cannot mark a `@test` function",
        ),
        (
            "@track_caller(depth: 1)\nfunc f() { }\nfunc main() -> i32 { 0 }\n",
            "takes no arguments",
        ),
    ] {
        let err = rejected("track_caller.nr", source);
        assert!(err.contains(problem), "{problem}: {err}");
    }
}
