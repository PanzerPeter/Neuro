// Integration tests for type inference (numeric literal inference, semantic analysis).
//
// The first group checks what the type checker accepts and rejects, so it runs
// `neurc check` and never reaches the backend. The second group compiles and runs a
// program, because its subject is the LLVM slot a declared type produces, which type
// checking alone cannot observe.
mod common;

use common::CompileTest;

#[test]
fn i64_annotation_accepts_a_small_literal() {
    let test = CompileTest::new();
    test.check(
        "i64_variable.nr",
        r#"func main() -> i32 {
    val x: i64 = 42
    return 42
}
"#,
    )
    .expect("42 should infer as i64 from the annotation");
}

#[test]
fn u32_parameter_types_its_argument_literal() {
    let test = CompileTest::new();
    test.check(
        "u32_function_param.nr",
        r#"func foo(x: u32) -> u32 {
    x
}

func main() -> i32 {
    val result: u32 = foo(100)
    return 100
}
"#,
    )
    .expect("100 should infer as u32 from the parameter");
}

#[test]
fn return_type_types_a_tail_expression_literal() {
    let test = CompileTest::new();
    test.check(
        "i16_return.nr",
        r#"func foo() -> i16 {
    256
}

func main() -> i32 {
    val result: i16 = foo()
    return 0
}
"#,
    )
    .expect("256 should infer as i16 from the return type");
}

#[test]
fn f32_annotation_accepts_a_float_literal() {
    let test = CompileTest::new();
    test.check(
        "f32_variable.nr",
        r#"func main() -> i32 {
    val x: f32 = 3.14
    return 0
}
"#,
    )
    .expect("3.14 should infer as f32, not f64");
}

#[test]
fn an_unannotated_integer_literal_defaults_to_i32() {
    let test = CompileTest::new();
    test.check(
        "default_i32.nr",
        r#"func main() -> i32 {
    val x = 42
    return 42
}
"#,
    )
    .expect("an unannotated literal should default to i32");
}

#[test]
fn several_widths_infer_independently_in_one_program() {
    let test = CompileTest::new();
    test.check(
        "mixed_types.nr",
        r#"func add_i16(a: i16, b: i16) -> i16 {
    a + b
}

func add_u64(a: u64, b: u64) -> u64 {
    a + b
}

func main() -> i32 {
    val x: i16 = 3
    val y: i16 = add_i16(x, 4)

    val a: u64 = 100
    val b: u64 = add_u64(a, 200)

    return 7
}
"#,
    )
    .expect("i16 and u64 inference should not interfere");
}

#[test]
fn a_literal_too_large_for_i8_is_rejected() {
    let test = CompileTest::new();
    let error = test
        .check(
            "i8_out_of_range.nr",
            r#"func main() -> i32 {
    val x: i8 = 300
    return 0
}
"#,
        )
        .expect_err("300 does not fit in i8");
    assert!(
        error.contains("out of range"),
        "the diagnostic should say the literal is out of range: {error}"
    );
}

#[test]
fn a_literal_too_large_for_u32_is_rejected() {
    let test = CompileTest::new();
    let error = test
        .check(
            "u32_out_of_range.nr",
            r#"func main() -> i32 {
    val x: u32 = 5000000000
    return 0
}
"#,
        )
        .expect_err("5000000000 exceeds u32::MAX");
    assert!(
        error.contains("out of range"),
        "the diagnostic should say the literal is out of range: {error}"
    );
}

// ── Codegen regression tests ──────────────────────────────────────────────────
// These tests exercise full compilation + execution to validate that declared
// type annotations are honoured at the LLVM IR level, not just semantically.

mod codegen_regressions {
    use super::common::CompileTest;

    #[test]
    fn regression_i64_annotation_creates_i64_alloca() {
        // val x: i64 = 255 previously created an i32 alloca.  Values that fit in
        // i32 silently gave correct results; the bug manifested when operations on
        // two annotated-i64 variables were passed to an i64-typed function.
        let test = CompileTest::new();
        let source = r#"
func take_i64(n: i64) -> i64 { return n }
func main() -> i32 {
    val a: i64 = 200
    val b: i64 = 55
    val c: i64 = take_i64(a + b)
    return c as i32
}
"#;
        let exit_code = test
            .compile_and_run("i64_alloca_regression.nr", source)
            .expect("Compilation or execution failed");
        assert_eq!(exit_code, 255, "Expected 255 (200 + 55)");
    }

    #[test]
    fn regression_f32_annotation_truncates_f64_literal() {
        // Float literals always default to f64; val x: f32 = 3.0 previously stored
        // an f64 value in an f64 alloca, silently ignoring the f32 annotation.
        let test = CompileTest::new();
        let source = r#"
func main() -> i32 {
    val x: f32 = 3.0
    val y: f32 = 2.0
    return (x + y) as i32
}
"#;
        let exit_code = test
            .compile_and_run("f32_annotation_regression.nr", source)
            .expect("Compilation or execution failed");
        assert_eq!(exit_code, 5, "Expected 5 (3.0 + 2.0 as i32)");
    }

    #[test]
    fn regression_i64_literal_in_binary_expression() {
        // Literals in binary expressions (not VarDecl) also defaulted to i32.
        // `i64_var - large_literal` caused an LLVM verifier type mismatch.
        let test = CompileTest::new();
        let source = r#"
func main() -> i32 {
    val a: i64 = 200
    val result: i64 = a + 55
    return result as i32
}
"#;
        let exit_code = test
            .compile_and_run("i64_binary_literal_regression.nr", source)
            .expect("Compilation or execution failed");
        assert_eq!(exit_code, 255, "Expected 255 (200 + 55)");
    }
}
