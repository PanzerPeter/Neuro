// End-to-end tests for out-of-range float-to-integer casts.
//
// `fptosi` / `fptoui` are defined only when the truncated value fits the target and
// yield `poison` otherwise, which made an out-of-range cast's result a function of the
// optimizer rather than of the source: the same program printed `-2147483648` at `-O0`,
// nothing at all at `-O3`, and stack garbage for a NaN. The cast now lowers through the
// saturating intrinsics, so every input has one defined answer.
//
// Each program is compiled at both `-O0` and `-O3` and the exit codes must agree: the
// disagreement is the defect, independently of which answer is the right one.
use crate::compile_harness::compile_tagged;
use std::process::Command;

/// Compile `source` at optimization level `opt` and return its exit code.
fn compile_and_run(source: &str, tag: &str, opt: &str) -> i32 {
    let exe = compile_tagged(source, &format!("f2i_{tag}_{opt}"), opt);
    Command::new(&exe)
        .output()
        .expect("run executable")
        .status
        .code()
        .unwrap_or(-1)
}

/// Run `source` at `-O0` and `-O3`, assert both give `expected`, and return it.
fn assert_same_at_every_opt_level(source: &str, tag: &str, expected: i32) {
    let debug = compile_and_run(source, tag, "0");
    let release = compile_and_run(source, tag, "3");
    assert_eq!(
        debug, release,
        "-O0 gave {debug} and -O3 gave {release} for the same program"
    );
    assert_eq!(debug, expected);
}

#[test]
fn regression_out_of_range_float_to_int_cast_saturates_instead_of_poison() {
    // A `f64` far past `i32::MAX` saturates to `i32::MAX`, and its negation to
    // `i32::MIN`. `+ 1` on the second brings the count into an exit-code range.
    assert_same_at_every_opt_level(
        r#"
func main() -> i32 {
    mut big: f64 = 1e300
    mut small: f64 = 0.0f64 - 1e300
    mut count: i32 = 0
    if (big as i32) == 2147483647 {
        count = count + 1
    }
    if (small as i32) == -2147483648 {
        count = count + 1
    }
    if (big as u32) == 4294967295u32 {
        count = count + 1
    }
    if (big as i64) == 9223372036854775807i64 {
        count = count + 1
    }
    return count
}
"#,
        "saturates",
        4,
    );
}

#[test]
fn regression_nan_float_to_int_cast_is_zero() {
    assert_same_at_every_opt_level(
        r#"
func main() -> i32 {
    mut nan: f64 = 0.0f64 / 0.0f64
    mut count: i32 = 0
    if (nan as i32) == 0 {
        count = count + 1
    }
    if (nan as u8) == 0u8 {
        count = count + 1
    }
    return count
}
"#,
        "nan",
        2,
    );
}

#[test]
fn regression_in_range_float_to_int_cast_still_truncates_toward_zero() {
    // The saturating intrinsic must not change the defined case: truncation toward
    // zero, which rounds a negative value up rather than down.
    assert_same_at_every_opt_level(
        r#"
func main() -> i32 {
    mut a: f64 = 3.9f64
    mut b: f64 = 0.0f64 - 3.9f64
    mut count: i32 = 0
    if (a as i32) == 3 {
        count = count + 1
    }
    if (b as i32) == -3 {
        count = count + 1
    }
    return count
}
"#,
        "truncates",
        2,
    );
}

#[test]
fn regression_folded_and_runtime_float_to_int_casts_agree() {
    // The same out-of-range cast written as a `const` (folded in the backend) and
    // through a mutable binding (lowered as an instruction) must give one answer.
    assert_same_at_every_opt_level(
        r#"
const BIG: f64 = 1e300

func main() -> i32 {
    mut same: f64 = 1e300
    if (BIG as i32) == (same as i32) {
        return 1
    }
    return 0
}
"#,
        "fold_agrees",
        1,
    );
}
