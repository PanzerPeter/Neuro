// `@test` and `neurc test` (§8.4): discovery across modules, one process per test, a panic
// failing only its own test, `std::testing`, and the forms the attribute refuses.

use crate::compile_harness::{CompileTest, assert_check_error, neurc_path, run};
use std::process::{Command, Output};

/// Run `neurc test` on the program laid out from `files`, rooted at the first.
fn neurc_test(files: &[(&str, &str)], args: &[&str]) -> Output {
    let test = CompileTest::new();
    let paths: Vec<_> = files
        .iter()
        .map(|(path, source)| test.write_source(path, source))
        .collect();
    Command::new(neurc_path())
        .arg("test")
        .arg(&paths[0])
        .args(args)
        .output()
        .expect("run neurc test")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n")
}

#[test]
fn a_panicking_test_fails_alone_and_shows_what_it_wrote() {
    let output = neurc_test(
        &[(
            "calc.nr",
            r#"
func add(a: i32, b: i32) -> i32 { a + b }

@test
func adds() {
    assert(add(2, 3) == 5)
}

@test
func wrong_sum() {
    println("checking 1 + 1")
    assert(add(1, 1) == 3)
}

@test
func out_of_bounds() {
    val xs = [1, 2, 3]
    val i = 3
    println("{xs[i]}")
}

@test
func still_runs() {
    println("never shown: a passing test's output is dropped")
}
"#,
        )],
        &[],
    );
    let text = stdout(&output);
    assert_eq!(output.status.code(), Some(1), "{text}");
    for line in [
        "running 4 test(s)",
        "test adds ... ok",
        "test wrong_sum ... FAILED",
        "test out_of_bounds ... FAILED",
        "test still_runs ... ok",
        "---- wrong_sum ----\nchecking 1 + 1\nassertion failed at ",
        "---- out_of_bounds ----\npanic: array index out of bounds at ",
        "test result: FAILED. 2 passed; 2 failed",
    ] {
        assert!(text.contains(line), "missing {line:?} in:\n{text}");
    }
    assert!(!text.contains("never shown"), "{text}");
}

#[test]
fn tests_are_found_in_every_module_and_need_no_main() {
    let output = neurc_test(
        &[
            (
                "root.nr",
                r#"
import ./shapes

@test
func uses_a_module() {
    assert(shapes::area(2, 4) == 8)
}

module inner {
    @test
    func in_a_block() {
        assert(true)
    }
}
"#,
            ),
            (
                "shapes.nr",
                r#"
export func area(w: i32, h: i32) -> i32 { w * h }

@test
func square() {
    assert(area(3, 3) == 9)
}
"#,
            ),
        ],
        &[],
    );
    let text = stdout(&output);
    assert!(output.status.success(), "{text}");
    for line in [
        "running 3 test(s)",
        "test uses_a_module ... ok",
        "test inner::in_a_block ... ok",
        "test shapes::square ... ok",
        "test result: ok. 3 passed; 0 failed",
    ] {
        assert!(text.contains(line), "missing {line:?} in:\n{text}");
    }
}

#[test]
fn a_test_may_call_the_program_main_and_other_commands_ignore_tests() {
    let source = r#"
func main() -> i32 {
    println("main ran")
    7
}

@test
func main_returns_seven() {
    assert(main() == 7)
}
"#;
    let output = neurc_test(&[("prog.nr", source)], &[]);
    assert!(output.status.success(), "{}", stdout(&output));
    // The executable `compile` and `run` build still enters at `main`.
    assert_eq!(run("prog.nr", source), 7);
}

#[test]
fn overflow_fails_a_debug_test_and_wraps_in_a_release_one() {
    let source = r#"
@test
func overflows() {
    mut x: i32 = 2147483647
    x += 1
    assert(x < 0)
}
"#;
    let debug = neurc_test(&[("wrap.nr", source)], &[]);
    assert!(
        stdout(&debug).contains("panic: integer overflow"),
        "{}",
        stdout(&debug)
    );
    let release = neurc_test(&[("wrap.nr", source)], &["-O2"]);
    assert!(release.status.success(), "{}", stdout(&release));
}

#[test]
fn std_testing_compares_floats_within_a_tolerance() {
    let output = neurc_test(
        &[(
            "close.nr",
            r#"
import std::testing::{assert_close}

@test
func sums_are_close() {
    assert_close(0.1 + 0.2, 0.3, 0.000000001)
    // Relative above 1: 0.5 off 1000.5 is within 0.1 percent.
    assert(std::testing::is_close(1000.0, 1000.5, 0.001))
    // Absolute below 1.
    assert(!std::testing::is_close(0.001, 0.002, 0.0001))
}

@test
func infinities_and_nan() {
    val inf: f64 = 1.0e308 * 10.0
    assert(std::testing::is_close(inf, inf, 0.0))
    assert(!std::testing::is_close(5.0, inf, 1.0))
    assert(!std::testing::is_close(-inf, inf, 1.0))
    val nan = inf - inf
    assert(!std::testing::is_close(nan, nan, 1.0))
}

@test
func not_close() {
    assert_close(1.0, 1.5, 0.1)
}
"#,
        )],
        &[],
    );
    let text = stdout(&output);
    for line in [
        "test sums_are_close ... ok",
        "test infinities_and_nan ... ok",
        "test not_close ... FAILED",
        "panic: assert_close: 1.0 is not within 0.1 of 1.5 at <std>/testing.nr:",
    ] {
        assert!(text.contains(line), "missing {line:?} in:\n{text}");
    }
}

/// One verdict reached four ways: through a tail `if`, a `val` binding, a `match` arm, and
/// a closure handed to a generic helper. Each pair holds one form that passes and one that
/// must fail, so a form that loses the assertion shows up as a wrong verdict.
#[test]
fn a_verdict_agrees_across_equivalent_forms() {
    let output = neurc_test(
        &[(
            "forms.nr",
            r#"
func mean(xs: [i32; 4]) -> i32 { (xs[0] + xs[1] + xs[2] + xs[3]) / 4 }

func holds<T>(value: T, check: (T) -> bool) { assert(check(value)) }

@test
func tail_if_pass() { if mean([1, 2, 3, 6]) == 3 { assert(true) } else { assert(false) } }
@test
func tail_if_fail() { if mean([1, 2, 3, 6]) == 3 { assert(false) } else { assert(true) } }

@test
func bound_pass() {
    val ok = mean([2, 2, 4, 4]) == 3
    assert(ok)
}
@test
func bound_fail() {
    val ok = mean([2, 2, 4, 4]) == 4
    assert(ok)
}

@test
func match_pass() {
    match mean([0, 0, 0, 12]) {
        3 => assert(true),
        _ => assert(false)
    }
}
@test
func match_fail() {
    match mean([0, 0, 0, 12]) {
        4 => assert(true),
        _ => assert(false)
    }
}

@test
func closure_pass() { holds(mean([3, 3, 3, 3]), |m: i32| m == 3) }
@test
func closure_fail() { holds(mean([3, 3, 3, 3]), |m: i32| m != 3) }
"#,
        )],
        &[],
    );
    let text = stdout(&output);
    for form in ["tail_if", "bound", "match", "closure"] {
        assert!(
            text.contains(&format!("test {form}_pass ... ok")),
            "{form}:\n{text}"
        );
        assert!(
            text.contains(&format!("test {form}_fail ... FAILED")),
            "{form}:\n{text}"
        );
    }
    assert!(text.contains("4 passed; 4 failed"), "{text}");
}

#[test]
fn test_rejects_a_signature_the_harness_cannot_call() {
    assert_check_error(
        "params.nr",
        "@test\nfunc t(x: i32) {\n    assert(x == 0)\n}\n",
        "`@test` function takes no parameters, found 1",
    );
    assert_check_error(
        "returns.nr",
        "@test\nfunc t() -> i32 {\n    0\n}\n",
        "`@test` function returns nothing, found `i32`",
    );
    assert_check_error(
        "method.nr",
        "struct S { x: i32 }\nimpl S {\n    @test\n    func t(&self) {\n    }\n}\n",
        "`@test` marks a free function, not a method",
    );
    // `neurc test` refuses it the same way, before anything runs.
    let output = neurc_test(&[("bad.nr", "@test\nfunc t<T>() {\n}\n")], &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("`@test` function may not be generic")
    );
}

/// The example and the showcase that carry `@test` functions pass them. The example harness
/// runs only their `main`.
#[test]
fn the_examples_with_tests_pass_them() {
    let examples = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    for program in ["basics/unit_tests.nr", "showcase/running_stats.nr"] {
        let output = Command::new(neurc_path())
            .arg("test")
            .arg(examples.join(program))
            .output()
            .expect("run neurc test");
        let text = stdout(&output);
        assert!(output.status.success(), "{program}:\n{text}");
        assert!(text.contains("3 passed; 0 failed"), "{program}:\n{text}");
    }
}
