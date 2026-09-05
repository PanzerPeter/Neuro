// End-to-end tests for the guards integer `/` and `%` carry.
//
// Two operand pairs are undefined behaviour for LLVM's division instructions: a zero
// divisor, and `MIN / -1`. Unguarded they do not merely give a surprising answer — at
// `-O0` the process dies of `SIGFPE` with nothing printed, and at `-O1` and above the
// optimizer folds the surrounding code around a poison value and the program prints
// garbage and carries on. Both are checked here at both ends of the optimization range,
// because the two builds fail differently and a guard that only holds in one is no
// guard at all.
//
// The zero divisor panics in every build; `MIN / -1` is an integer overflow and so
// follows the same rule the arithmetic operators do — a panic in debug builds, the
// two's-complement wrap in release.
use std::path::PathBuf;
use std::process::{Command, Output};

fn neurc_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_neurc"))
}

/// Compile `source` at optimization level `opt` and run it, returning the process
/// result. Every divisor below is read out of a `mut` binding so that constant folding
/// cannot answer the question before the guard is reached.
fn compile_and_run(source: &str, tag: &str, opt: &str) -> Output {
    let dir = std::env::temp_dir();
    let src = dir.join(format!("neuro_division_{tag}_{opt}.nr"));
    let exe = dir.join(format!("neuro_division_{tag}_{opt}"));
    std::fs::write(&src, source).expect("write source");

    let compiled = Command::new(neurc_path())
        .args(["compile"])
        .arg(&src)
        .args(["-O", opt, "-o"])
        .arg(&exe)
        .output()
        .expect("run neurc");
    assert!(
        compiled.status.success(),
        "compile failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&compiled.stderr)
    );

    Command::new(&exe).output().expect("run executable")
}

fn assert_panicked_with(output: &Output, message: &str) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(message),
        "expected a `{message}` diagnostic, got status {:?} and stderr:\n{stderr}",
        output.status
    );
}

const DIVIDE_BY_ZERO: &str = r#"
func main() -> i32 {
    mut a: i32 = 10
    mut b: i32 = 0
    return a / b
}
"#;

const REMAINDER_BY_ZERO: &str = r#"
func main() -> i32 {
    mut a: u8 = 10
    mut b: u8 = 0
    return (a % b) as i32
}
"#;

/// `i32::MIN` has no literal spelling, so it is reached by a subtraction the checker
/// can range-check.
const MIN_OVER_MINUS_ONE: &str = r#"
func main() -> i32 {
    mut a: i32 = -2147483647
    a = a - 1
    mut b: i32 = -1
    val q = a / b
    println("{q}")
    return 0
}
"#;

const MIN_REM_MINUS_ONE: &str = r#"
func main() -> i32 {
    mut a: i32 = -2147483647
    a = a - 1
    mut b: i32 = -1
    val r = a % b
    println("{r}")
    return 0
}
"#;

#[test]
fn a_zero_divisor_panics_in_every_build() {
    for opt in ["0", "3"] {
        assert_panicked_with(
            &compile_and_run(DIVIDE_BY_ZERO, "div", opt),
            "panic: division by zero",
        );
        assert_panicked_with(
            &compile_and_run(REMAINDER_BY_ZERO, "rem", opt),
            "panic: remainder by zero",
        );
    }
}

#[test]
fn min_over_minus_one_panics_in_debug_builds() {
    assert_panicked_with(
        &compile_and_run(MIN_OVER_MINUS_ONE, "ovf", "0"),
        "panic: integer overflow",
    );
    assert_panicked_with(
        &compile_and_run(MIN_REM_MINUS_ONE, "ovfrem", "0"),
        "panic: integer overflow",
    );
}

#[test]
fn min_over_minus_one_wraps_in_release_builds() {
    // The wrapped quotient is `MIN` itself and the wrapped remainder is `0` — the same
    // answers two's complement gives, reached without handing `-1` to the instruction.
    let quotient = compile_and_run(MIN_OVER_MINUS_ONE, "ovf", "3");
    assert!(quotient.status.success(), "release build must not abort");
    assert_eq!(
        String::from_utf8_lossy(&quotient.stdout).trim_end(),
        "-2147483648"
    );

    let remainder = compile_and_run(MIN_REM_MINUS_ONE, "ovfrem", "3");
    assert!(remainder.status.success(), "release build must not abort");
    assert_eq!(String::from_utf8_lossy(&remainder.stdout).trim_end(), "0");
}

#[test]
fn an_ordinary_division_is_unaffected() {
    // Truncation toward zero, in both builds, with the guards in place.
    let source = r#"
func main() -> i32 {
    mut a: i64 = -9
    mut b: i64 = 4
    mut u: u32 = 7
    mut v: u32 = 2
    println("{a / b} {a % b} {u / v} {u % v}")
    return 0
}
"#;
    for opt in ["0", "3"] {
        let output = compile_and_run(source, "plain", opt);
        assert!(output.status.success(), "plain division must not abort");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim_end(),
            "-2 -1 3 1",
            "at -O{opt}"
        );
    }
}
