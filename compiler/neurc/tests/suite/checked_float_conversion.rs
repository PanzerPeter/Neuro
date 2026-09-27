// End-to-end tests for checked float-to-integer conversion: `.to_checked::<T>()`, which
// answers `None` where `as` would saturate or map NaN to zero, and the warning `as` gets
// when its operand is a constant it will saturate.
use crate::compile_harness::CompileTest;
use std::path::PathBuf;
use std::process::Command;

/// A program whose `main` counts how many of `checks` hold, one `if` each.
fn counting_program(prelude: &str, checks: &[&str]) -> String {
    let body: String = checks
        .iter()
        .map(|check| format!("    if {check} {{\n        count += 1\n    }}\n"))
        .collect();
    format!(
        "{prelude}\nfunc main() -> i32 {{\n    mut count: i32 = 0\n{body}    return count\n}}\n"
    )
}

const HELPERS: &str = r#"
func some_i8(o: Option<i8>, want: i8) -> bool {
    match o {
        Option::Some(v) => v == want,
        Option::None => false,
    }
}
func none_i8(o: Option<i8>) -> bool {
    match o {
        Option::Some(_) => false,
        Option::None => true,
    }
}
func some_i64(o: Option<i64>) -> bool {
    match o {
        Option::Some(_) => true,
        Option::None => false,
    }
}
func some_u64(o: Option<u64>) -> bool {
    match o {
        Option::Some(_) => true,
        Option::None => false,
    }
}
func some_u32(o: Option<u32>) -> bool {
    match o {
        Option::Some(_) => true,
        Option::None => false,
    }
}
"#;

#[test]
fn to_checked_truncates_in_range_and_refuses_out_of_range() {
    // Read through `mut` bindings so nothing is folded before the run-time path sees it.
    let prelude = format!(
        "{HELPERS}
func v(x: f64) -> f64 {{
    mut y = x
    y
}}"
    );
    let checks = [
        "some_i8(v(127.9).to_checked::<i8>(), 127)",
        "some_i8(v(-128.0).to_checked::<i8>(), -128)",
        "some_i8(v(-128.9).to_checked::<i8>(), -128)",
        "some_i8(v(-0.5).to_checked::<i8>(), 0)",
        "none_i8(v(128.0).to_checked::<i8>())",
        "none_i8(v(-129.0).to_checked::<i8>())",
        "none_i8((v(0.0) / v(0.0)).to_checked::<i8>())",
        "none_i8((v(1.0) / v(0.0)).to_checked::<i8>())",
        "none_i8((v(-1.0) / v(0.0)).to_checked::<i8>())",
    ];
    let test = CompileTest::new();
    let exit = test
        .compile_and_run("to_checked_i8.nr", &counting_program(&prelude, &checks))
        .expect("to_checked i8 program failed");
    assert_eq!(exit, checks.len() as i32);
}

#[test]
fn to_checked_is_exact_at_the_64_bit_bounds() {
    // `i64::MAX` and `u64::MAX` are not floats: both round up to the next power of two,
    // one past the type, so the float nearest the maximum is refused.
    let checks = [
        "!some_i64(v(9223372036854775807.0).to_checked::<i64>())",
        "some_i64(v(-9223372036854775808.0).to_checked::<i64>())",
        "some_i64(v(9223372036854774784.0).to_checked::<i64>())",
        "!some_u64(v(18446744073709551615.0).to_checked::<u64>())",
        "some_u64(v(18446744073709549568.0).to_checked::<u64>())",
        "!some_u64(v(-1.0).to_checked::<u64>())",
        "some_u32(w(4294967040.0).to_checked::<u32>())",
        "!some_u32(w(4294967295.0).to_checked::<u32>())",
    ];
    let prelude = format!(
        "{HELPERS}
func v(x: f64) -> f64 {{
    mut y = x
    y
}}
func w(x: f32) -> f32 {{
    mut y = x
    y
}}"
    );
    let test = CompileTest::new();
    let exit = test
        .compile_and_run("to_checked_wide.nr", &counting_program(&prelude, &checks))
        .expect("to_checked 64-bit program failed");
    assert_eq!(exit, checks.len() as i32);
}

#[test]
fn to_checked_result_feeds_coalesce_and_match() {
    let source = r#"
func main() -> i32 {
    val scale = |x: f64| -> Option<u8> { (x * 10.0).to_checked::<u8>() }
    val a = scale(4.2) ?? 0u8
    val b = scale(99.0) ?? 7u8
    return (a as i32) + (b as i32)
}
"#;
    let test = CompileTest::new();
    let exit = test
        .compile_and_run("to_checked_coalesce.nr", source)
        .expect("to_checked coalesce program failed");
    assert_eq!(exit, 42 + 7);
}

#[test]
fn to_checked_without_a_target_is_rejected() {
    let source = r#"
func main() -> i32 {
    val x: f64 = 1.5
    val r = x.to_checked()
    return 0
}
"#;
    let test = CompileTest::new();
    let err = test
        .check("to_checked_no_target.nr", source)
        .expect_err("a `.to_checked` with no turbofish must not type-check");
    assert!(err.contains("to_checked"), "unexpected diagnostic: {err}");
}

#[test]
fn to_checked_to_a_float_is_rejected() {
    let source = r#"
func main() -> i32 {
    val x: f64 = 1.5
    val r = x.to_checked::<f32>()
    return 0
}
"#;
    let test = CompileTest::new();
    let err = test
        .check("to_checked_float_target.nr", source)
        .expect_err("a float target must not type-check");
    assert!(err.contains("is not one"), "unexpected diagnostic: {err}");
}

/// `neurc check` on `source`, returning its stderr after asserting success.
fn check_stderr(name: &str, source: &str) -> String {
    let test = CompileTest::new();
    let path = test.write_source(name, source);
    let output = Command::new(PathBuf::from(env!("CARGO_BIN_EXE_neurc")))
        .arg("check")
        .arg(&path)
        .output()
        .expect("run neurc");
    assert!(output.status.success(), "a warning must not fail the check");
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn constant_out_of_range_cast_warns_and_still_compiles() {
    let stderr = check_stderr(
        "cast_warning.nr",
        r#"
const LIMIT: f64 = 1e10
func main() -> i32 {
    val a: i32 = LIMIT as i32
    val b: u8 = 200.0 as u8
    return b as i32
}
"#,
    );
    assert_eq!(
        stderr.matches("warning[float-cast-out-of-range]").count(),
        1,
        "exactly the `LIMIT` cast should warn: {stderr}"
    );
}

#[test]
fn allow_silences_the_constant_cast_warning() {
    let stderr = check_stderr(
        "cast_warning_allowed.nr",
        r#"
@allow(float_cast_out_of_range)
func main() -> i32 {
    val a: i32 = 1e20 as i32
    return 0
}
"#,
    );
    assert!(!stderr.contains("float-cast-out-of-range"), "{stderr}");
}
