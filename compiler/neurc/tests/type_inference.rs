// NEURO Compiler - Integration tests for type inference
mod common;
// Tests for numeric literal type inference feature (semantic analysis)
//
// NOTE: These tests focus on type checking behavior. Full code generation
// with inferred types requires passing type information from semantic analysis
// to LLVM backend, which is deferred to a future phase.

use std::path::PathBuf;
use std::process::Command;

fn get_test_dir() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop(); // Go to compiler dir
    path.pop(); // Go to project root
    path.push("tests");
    path.push("type_inference");
    path
}

/// Check a test file for type errors (using neurc check command)
fn check_test(test_name: &str) -> (bool, String) {
    let test_dir = get_test_dir();
    let source_file = test_dir.join(format!("{}.nr", test_name));

    // Run type checking
    let check_result = Command::new("cargo")
        .args([
            "run",
            "-p",
            "neurc",
            "--",
            "check",
            source_file.to_str().unwrap(),
        ])
        .output()
        .expect("Failed to run compiler");

    let check_success = check_result.status.success();
    let check_output = String::from_utf8_lossy(&check_result.stderr).to_string();

    (check_success, check_output)
}

#[test]
fn test_i64_inference_in_variable() {
    // Test: val x: i64 = 42
    // Literal 42 should infer as i64 and pass type checking
    let (success, output) = check_test("i64_variable");
    assert!(success, "Type checking failed: {}", output);
}

#[test]
fn test_u32_inference_in_function_param() {
    // Test: func foo(x: u32) -> u32 { x } ... foo(100)
    // Literal 100 should infer as u32 and pass type checking
    let (success, output) = check_test("u32_function_param");
    assert!(success, "Type checking failed: {}", output);
}

#[test]
fn test_i16_inference_in_return() {
    // Test: func foo() -> i16 { 256 }
    // Literal 256 should infer as i16 and pass type checking
    let (success, output) = check_test("i16_return");
    assert!(success, "Type checking failed: {}", output);
}

#[test]
fn test_f32_inference_in_variable() {
    // Test: val x: f32 = 3.14
    // Literal 3.14 should infer as f32 and pass type checking
    let (success, output) = check_test("f32_variable");
    assert!(success, "Type checking failed: {}", output);
}

#[test]
fn test_i8_out_of_range_error() {
    // Test: val x: i8 = 300
    // Should produce type error - 300 doesn't fit in i8
    let (success, output) = check_test("i8_out_of_range");

    assert!(!success, "Should have failed type checking");
    assert!(
        output.contains("out of range") || output.contains("OutOfRange"),
        "Error message should mention out of range: {}",
        output
    );
}

#[test]
fn test_u32_out_of_range_error() {
    // Test: val x: u32 = 5000000000
    // Should produce type error - value too large for u32 (max 4294967295)
    let (success, output) = check_test("u32_negative");

    assert!(!success, "Should have failed type checking");
    assert!(
        output.contains("out of range") || output.contains("OutOfRange"),
        "Error message should mention out of range: {}",
        output
    );
}

#[test]
fn test_default_to_i32() {
    // Test: val x = 42 (no type annotation)
    // Should default to i32 and pass type checking
    let (success, output) = check_test("default_i32");
    assert!(success, "Type checking failed: {}", output);
}

#[test]
fn test_mixed_types_inference() {
    // Test: Complex program with multiple type inferences
    // All literals should infer correctly and pass type checking
    let (success, output) = check_test("mixed_types");
    assert!(success, "Type checking failed: {}", output);
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
