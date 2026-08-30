// Float primitive methods — `.is_nan()` on `f32` / `f64` (Phase 2A).
//
// End-to-end coverage: `.is_nan()` dispatches through the builtin-method intrinsic path
// and lowers to an unordered self-comparison (`fcmp uno`). NaN is the only value it
// answers `true` for, which is what makes it necessary — `x != x` is false for NaN too,
// so the test is not expressible with the comparison operators.
mod common;
use common::CompileTest;

#[test]
fn is_nan_detects_nan_and_rejects_ordinary_values() {
    let test = CompileTest::new();
    // Each branch contributes a distinct digit, so a wrong answer is identifiable
    // from the exit code alone: only the NaN branch may fire.
    let source = r#"
func main() -> i32 {
    val zero: f64 = 0.0
    val nan: f64 = zero / zero
    val inf: f64 = 1.0 / zero

    mut code: i32 = 0
    if nan.is_nan() { code += 1 }
    if inf.is_nan() { code += 10 }
    if (0.0 - inf).is_nan() { code += 100 }
    if (1.5).is_nan() { code += 1000 }
    return code
}
"#;
    let exit = test
        .compile_and_run("is_nan_f64.nr", source)
        .expect("is_nan compilation or execution failed");
    assert_eq!(exit, 1);
}

#[test]
fn is_nan_works_on_f32() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val zero: f32 = 0.0f32
    val nan: f32 = zero / zero
    val ok: f32 = 2.5f32

    if ok.is_nan() { return 1 }
    if nan.is_nan() { return 7 }
    return 2
}
"#;
    let exit = test
        .compile_and_run("is_nan_f32.nr", source)
        .expect("is_nan on f32 compilation or execution failed");
    assert_eq!(exit, 7);
}

#[test]
fn is_nan_result_composes_with_boolean_operators() {
    let test = CompileTest::new();
    // The result is an ordinary `bool`, so it feeds `&&` / `||` / `!` and a
    // function parameter like any other predicate.
    let source = r#"
func usable(x: f64, limit: f64) -> bool {
    return !x.is_nan() && x < limit
}

func main() -> i32 {
    val zero: f64 = 0.0
    val nan: f64 = zero / zero

    mut code: i32 = 0
    if usable(3.0, 10.0) { code += 5 }
    if usable(nan, 10.0) { code += 20 }
    if usable(50.0, 10.0) { code += 40 }
    return code
}
"#;
    let exit = test
        .compile_and_run("is_nan_bool.nr", source)
        .expect("is_nan boolean composition failed");
    assert_eq!(exit, 5);
}

#[test]
fn is_nan_on_integer_receiver_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val n: i32 = 3
    if n.is_nan() { return 1 }
    return 0
}
"#;
    let src = test.write_source("is_nan_int.nr", source);
    let err = test
        .compile(&src)
        .expect_err("an integer receiver must not resolve `.is_nan()`");
    assert!(
        err.contains("is_nan"),
        "diagnostic should name the missing method, got: {err}"
    );
}
