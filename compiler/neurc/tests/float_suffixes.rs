// Integration tests for float literal type suffixes (§1.2, §1.4)
mod common;
use common::CompileTest;

#[test]
fn suffix_f32_infers_without_annotation() {
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "float_suffix_f32.nr",
            r#"
func main() -> i32 {
    val x = 1.5f32
    val y = 0.0f32
    return 0
}
"#,
        )
        .expect("compilation failed");
    assert_eq!(exit, 0);
}

#[test]
fn suffix_f64_infers_without_annotation() {
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "float_suffix_f64.nr",
            r#"
func main() -> i32 {
    val x = 2.0f64
    val y = 3.141592653589793f64
    return 0
}
"#,
        )
        .expect("compilation failed");
    assert_eq!(exit, 0);
}

#[test]
fn suffix_consistent_with_explicit_annotation() {
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "float_suffix_annotated.nr",
            r#"
func main() -> i32 {
    val x: f32 = 1.5f32
    val y: f64 = 2.0f64
    return 0
}
"#,
        )
        .expect("compilation failed");
    assert_eq!(exit, 0);
}

#[test]
fn suffix_exponent_form_accepted() {
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "float_suffix_exponent.nr",
            r#"
func main() -> i32 {
    val a = 1e10f32
    val b = 1.5e-5f64
    return 0
}
"#,
        )
        .expect("compilation failed");
    assert_eq!(exit, 0);
}

#[test]
fn suffix_mismatch_with_annotation_rejected() {
    let test = CompileTest::new();
    let source_path = test.write_source(
        "float_suffix_mismatch.nr",
        r#"
func main() -> i32 {
    val x: f32 = 1.5f64
    return 0
}
"#,
    );
    assert!(
        test.compile(&source_path).is_err(),
        "expected type mismatch for f32 binding initialized with f64 literal"
    );
}

#[test]
fn unsuffixed_float_still_defaults_to_f64() {
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "float_unsuffixed.nr",
            r#"
func main() -> i32 {
    val x = 3.14
    val y: f64 = 2.5
    return 0
}
"#,
        )
        .expect("compilation failed");
    assert_eq!(exit, 0);
}
