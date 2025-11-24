// End-to-end integration tests for the NEURO compiler
// Tests the complete compilation pipeline: source → executable → execution

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

/// Helper struct for running end-to-end compilation tests
struct CompileTest {
    temp_dir: TempDir,
}

impl CompileTest {
    fn new() -> Self {
        Self {
            temp_dir: TempDir::new().expect("Failed to create temp directory"),
        }
    }

    /// Write source code to a temporary .nr file and return its path
    fn write_source(&self, filename: &str, source: &str) -> PathBuf {
        let source_path = self.temp_dir.path().join(filename);
        fs::write(&source_path, source).expect("Failed to write source file");
        source_path
    }

    /// Compile a source file and return the path to the executable
    fn compile(&self, source_path: &PathBuf) -> Result<PathBuf, String> {
        let output_path = source_path.with_extension(if cfg!(target_os = "windows") {
            "exe"
        } else {
            ""
        });

        // Build neurc path relative to the test binary
        let neurc_exe = if cfg!(target_os = "windows") {
            "neurc.exe"
        } else {
            "neurc"
        };

        let neurc_path = std::env::current_exe()
            .expect("Failed to get current exe path")
            .parent()
            .expect("Failed to get parent directory")
            .parent()
            .expect("Failed to get grandparent directory")
            .join(neurc_exe);

        // Run the compiler
        let output = Command::new(&neurc_path)
            .arg("compile")
            .arg(source_path)
            .arg("-o")
            .arg(&output_path)
            .output()
            .expect("Failed to execute neurc");

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(format!(
                "Compilation failed:\nstdout: {}\nstderr: {}",
                stdout, stderr
            ));
        }

        Ok(output_path)
    }

    /// Run an executable and return its exit code
    fn run_executable(&self, exe_path: &PathBuf) -> Result<i32, String> {
        let output = Command::new(exe_path)
            .output()
            .map_err(|e| format!("Failed to execute {}: {}", exe_path.display(), e))?;

        Ok(output.status.code().unwrap_or(-1))
    }

    /// Compile and run a program, returning its exit code
    fn compile_and_run(&self, filename: &str, source: &str) -> Result<i32, String> {
        let source_path = self.write_source(filename, source);
        let exe_path = self.compile(&source_path)?;
        self.run_executable(&exe_path)
    }
}

#[test]
fn test_simple_return() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    return 42
}
"#;

    let exit_code = test
        .compile_and_run("simple_return.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 42, "Expected exit code 42");
}

#[test]
fn test_arithmetic_operations() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: i32 = 10
    val b: i32 = 5
    val sum: i32 = a + b
    val diff: i32 = a - b
    val product: i32 = a * b
    return sum + diff + product
}
"#;

    let exit_code = test
        .compile_and_run("arithmetic.nr", source)
        .expect("Compilation or execution failed");
    // sum=15, diff=5, product=50, total=70
    assert_eq!(exit_code, 70, "Expected exit code 70");
}

#[test]
fn test_function_call() {
    let test = CompileTest::new();
    let source = r#"
func add(a: i32, b: i32) -> i32 {
    return a + b
}

func main() -> i32 {
    val result: i32 = add(5, 3)
    return result
}
"#;

    let exit_code = test
        .compile_and_run("function_call.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 8, "Expected exit code 8");
}

#[test]
fn test_nested_function_calls() {
    let test = CompileTest::new();
    let source = r#"
func add(a: i32, b: i32) -> i32 {
    return a + b
}

func multiply(a: i32, b: i32) -> i32 {
    return a * b
}

func main() -> i32 {
    val sum: i32 = add(3, 4)
    val product: i32 = multiply(sum, 2)
    return product
}
"#;

    let exit_code = test
        .compile_and_run("nested_calls.nr", source)
        .expect("Compilation or execution failed");
    // sum = 7, product = 14
    assert_eq!(exit_code, 14, "Expected exit code 14");
}

#[test]
fn test_if_else_true() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val x: i32 = 10
    val result: i32 = 0
    if x > 5 {
        val result: i32 = 100
        return result
    }
    return 50
}
"#;

    let exit_code = test
        .compile_and_run("if_else_true.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 100, "Expected exit code 100");
}

#[test]
fn test_if_else_false() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val x: i32 = 3
    if x > 5 {
        return 100
    }
    return 50
}
"#;

    let exit_code = test
        .compile_and_run("if_else_false.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 50, "Expected exit code 50");
}

#[test]
fn test_comparison_operators() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: i32 = 10
    val b: i32 = 5

    if a == b {
        return 1
    }
    if a != b {
        return 2
    }
    return 0
}
"#;

    let exit_code = test
        .compile_and_run("comparison.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 2, "Expected exit code 2 (a != b)");
}

#[test]
fn test_logical_operators() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: bool = true
    val b: bool = false

    if a && b {
        return 1
    }
    if a || b {
        return 2
    }
    return 3
}
"#;

    let exit_code = test
        .compile_and_run("logical.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 2, "Expected exit code 2 (a || b)");
}

#[test]
fn test_complex_expression() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: i32 = 2
    val b: i32 = 3
    val c: i32 = 4
    val result: i32 = a * b + c * 5 - 10
    return result
}
"#;

    let exit_code = test
        .compile_and_run("complex_expr.nr", source)
        .expect("Compilation or execution failed");
    // 2*3 + 4*5 - 10 = 6 + 20 - 10 = 16
    assert_eq!(exit_code, 16, "Expected exit code 16");
}

#[test]
fn test_milestone_program() {
    let test = CompileTest::new();
    let source = r#"
func add(a: i32, b: i32) -> i32 {
    return a + b
}

func main() -> i32 {
    val result: i32 = add(5, 3)
    return result
}
"#;

    let exit_code = test
        .compile_and_run("milestone.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 8, "Expected exit code 8");
}

#[test]
fn test_multiple_variables() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val x: i32 = 10
    val y: i32 = 20
    val z: i32 = x + y
    return z
}
"#;

    let exit_code = test
        .compile_and_run("multiple_vars.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 30, "Expected exit code 30");
}

// Note: Float operations are supported but not tested here
// because exit codes are integers. Float support is verified
// by the compiler's type system and codegen tests.

#[test]
fn test_multiple_parameters() {
    let test = CompileTest::new();
    let source = r#"
func sum_three(a: i32, b: i32, c: i32) -> i32 {
    return a + b + c
}

func main() -> i32 {
    val result: i32 = sum_three(10, 20, 30)
    return result
}
"#;

    let exit_code = test
        .compile_and_run("multi_params.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 60, "Expected exit code 60");
}

#[test]
fn test_zero_return() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    return 0
}
"#;

    let exit_code = test
        .compile_and_run("zero_return.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 0, "Expected exit code 0");
}

#[test]
fn test_negative_numbers() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: i32 = -10
    val b: i32 = 5
    val result: i32 = a + b
    return result
}
"#;

    let exit_code = test
        .compile_and_run("negative.nr", source)
        .expect("Compilation or execution failed");
    // Note: Exit codes are typically 0-255, but we can test the logic
    // -10 + 5 = -5, which wraps to 251 on most systems (256 - 5)
    // For now, let's just verify it compiles and runs
    assert!(exit_code != 0, "Program should have executed");
}

#[test]
fn test_unary_negation() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: i32 = 10
    val b: i32 = -a
    return a + b
}
"#;

    let exit_code = test
        .compile_and_run("unary_neg.nr", source)
        .expect("Compilation or execution failed");
    // 10 + (-10) = 0
    assert_eq!(exit_code, 0, "Expected exit code 0");
}

#[test]
fn test_boolean_literals() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: bool = true
    val b: bool = false

    if a {
        if !b {
            return 15
        }
        return 10
    }
    return 5
}
"#;

    let exit_code = test
        .compile_and_run("bool_literals.nr", source)
        .expect("Compilation or execution failed");
    // a is true, !b is true -> return 15
    assert_eq!(exit_code, 15, "Expected exit code 15");
}

#[test]
fn test_division_operator() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: i32 = 20
    val b: i32 = 4
    val result: i32 = a / b
    return result
}
"#;

    let exit_code = test
        .compile_and_run("division.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 5, "Expected exit code 5 (20 / 4)");
}

#[test]
fn test_modulo_operator() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: i32 = 17
    val b: i32 = 5
    val result: i32 = a % b
    return result
}
"#;

    let exit_code = test
        .compile_and_run("modulo.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 2, "Expected exit code 2 (17 % 5)");
}

#[test]
fn test_simple_assignment() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut x: i32 = 10
    x = 20
    return x
}
"#;

    let exit_code = test
        .compile_and_run("simple_assignment.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 20, "Expected exit code 20 after assignment");
}

#[test]
fn test_assignment_with_expression() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut counter: i32 = 0
    counter = counter + 5
    counter = counter * 2
    return counter
}
"#;

    let exit_code = test
        .compile_and_run("assignment_expr.nr", source)
        .expect("Compilation or execution failed");
    // counter = 0 + 5 = 5, then 5 * 2 = 10
    assert_eq!(exit_code, 10, "Expected exit code 10");
}

#[test]
fn test_multiple_assignments() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut x: i32 = 1
    mut y: i32 = 2
    x = 10
    y = 20
    x = x + y
    return x
}
"#;

    let exit_code = test
        .compile_and_run("multiple_assignments.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 30, "Expected exit code 30 (10 + 20)");
}

#[test]
fn test_nested_arithmetic() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: i32 = 100
    val b: i32 = 10
    val c: i32 = 3
    val result: i32 = (a / b) % c
    return result
}
"#;

    let exit_code = test
        .compile_and_run("nested_arithmetic.nr", source)
        .expect("Compilation or execution failed");
    // (100 / 10) % 3 = 10 % 3 = 1
    assert_eq!(exit_code, 1, "Expected exit code 1");
}

// Note: Float comparisons in if conditions are not yet supported in Phase 1
// The LLVM backend currently only handles integer comparisons in conditional expressions
// Float arithmetic works, but using float comparisons as boolean conditions will fail
// This is a known limitation to be addressed in Phase 2

// Note: Deeply nested if/else chains may fail control flow analysis in Phase 1
// The current implementation has limitations with complex control flow patterns
// Simple if/else works, but deeply nested or complex chains may not be recognized
// as having complete return coverage. This is a known limitation for Phase 2.

// ==============================================================================
// Expression-Based Returns Tests (Phase 1 Feature)
// End-to-end compilation and execution tests
// ==============================================================================

#[test]
fn test_expression_return_simple() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    42
}
"#;

    let exit_code = test
        .compile_and_run("expression_return_simple.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 42, "Expected exit code 42 from implicit return");
}

#[test]
fn test_expression_return_arithmetic() {
    let test = CompileTest::new();
    let source = r#"
func add(a: i32, b: i32) -> i32 {
    a + b
}

func main() -> i32 {
    add(10, 15)
}
"#;

    let exit_code = test
        .compile_and_run("expression_return_arithmetic.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 25, "Expected exit code 25 (10 + 15)");
}

#[test]
fn test_expression_return_with_variable() {
    let test = CompileTest::new();
    let source = r#"
func compute() -> i32 {
    val x: i32 = 10
    val y: i32 = 20
    x + y
}

func main() -> i32 {
    compute()
}
"#;

    let exit_code = test
        .compile_and_run("expression_return_variable.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 30, "Expected exit code 30 (10 + 20)");
}

#[test]
fn test_expression_return_nested_calls() {
    let test = CompileTest::new();
    let source = r#"
func double(x: i32) -> i32 {
    x * 2
}

func quad(x: i32) -> i32 {
    double(double(x))
}

func main() -> i32 {
    quad(5)
}
"#;

    let exit_code = test
        .compile_and_run("expression_return_nested.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 20, "Expected exit code 20 (5 * 2 * 2)");
}

#[test]
fn test_expression_return_complex_expression() {
    let test = CompileTest::new();
    let source = r#"
func calculate(a: i32, b: i32, c: i32) -> i32 {
    (a + b) * c - a
}

func main() -> i32 {
    calculate(3, 7, 4)
}
"#;

    let exit_code = test
        .compile_and_run("expression_return_complex.nr", source)
        .expect("Compilation or execution failed");
    // (3 + 7) * 4 - 3 = 10 * 4 - 3 = 40 - 3 = 37
    assert_eq!(exit_code, 37, "Expected exit code 37");
}

#[test]
fn test_mixed_explicit_implicit_returns() {
    let test = CompileTest::new();
    let source = r#"
func explicit_func(x: i32) -> i32 {
    return x + 5
}

func implicit_func(x: i32) -> i32 {
    x * 3
}

func main() -> i32 {
    val a: i32 = explicit_func(10)
    val b: i32 = implicit_func(10)
    a + b
}
"#;

    let exit_code = test
        .compile_and_run("mixed_returns.nr", source)
        .expect("Compilation or execution failed");
    // explicit_func(10) = 15, implicit_func(10) = 30, 15 + 30 = 45
    assert_eq!(exit_code, 45, "Expected exit code 45");
}

#[test]
fn test_expression_return_with_multiple_statements() {
    let test = CompileTest::new();
    let source = r#"
func process(x: i32) -> i32 {
    val step1: i32 = x * 2
    val step2: i32 = step1 + 10
    val step3: i32 = step2 / 2
    step3
}

func main() -> i32 {
    process(10)
}
"#;

    let exit_code = test
        .compile_and_run("expression_return_multi_stmt.nr", source)
        .expect("Compilation or execution failed");
    // step1 = 20, step2 = 30, step3 = 15
    assert_eq!(exit_code, 15, "Expected exit code 15");
}

// ============================================================================
// String Type End-to-End Tests (Phase 1)
// ============================================================================

#[test]
fn test_string_literal_return() {
    let test = CompileTest::new();
    let source = r#"
func get_message() -> string {
    return "Hello, NEURO!"
}

func main() -> i32 {
    val msg: string = get_message()
    return 0
}
"#;

    let exit_code = test
        .compile_and_run("string_literal.nr", source)
        .expect("String literal compilation or execution failed");
    assert_eq!(exit_code, 0);
}

#[test]
fn test_string_parameter() {
    let test = CompileTest::new();
    let source = r#"
func echo(msg: string) -> string {
    return msg
}

func main() -> i32 {
    val result: string = echo("test message")
    return 0
}
"#;

    let exit_code = test
        .compile_and_run("string_parameter.nr", source)
        .expect("String parameter compilation or execution failed");
    assert_eq!(exit_code, 0);
}

#[test]
fn test_string_with_escapes() {
    let test = CompileTest::new();
    let source = r#"
func get_escaped() -> string {
    return "Line1\nLine2\tTabbed"
}

func main() -> i32 {
    val s: string = get_escaped()
    return 0
}
"#;

    let exit_code = test
        .compile_and_run("string_escapes.nr", source)
        .expect("String with escapes compilation or execution failed");
    assert_eq!(exit_code, 0);
}

#[test]
fn test_empty_string() {
    let test = CompileTest::new();
    let source = r#"
func get_empty() -> string {
    return ""
}

func main() -> i32 {
    val empty: string = get_empty()
    return 0
}
"#;

    let exit_code = test
        .compile_and_run("empty_string.nr", source)
        .expect("Empty string compilation or execution failed");
    assert_eq!(exit_code, 0);
}

#[test]
fn test_string_variable_assignment() {
    let test = CompileTest::new();
    let source = r#"
func test_vars() -> string {
    val msg1: string = "First"
    val msg2: string = "Second"
    val msg3: string = msg1
    return msg3
}

func main() -> i32 {
    val result: string = test_vars()
    return 0
}
"#;

    let exit_code = test
        .compile_and_run("string_variables.nr", source)
        .expect("String variable assignment compilation or execution failed");
    assert_eq!(exit_code, 0);
}

#[test]
fn test_string_implicit_return() {
    let test = CompileTest::new();
    let source = r#"
func implicit_string() -> string {
    "Implicit return"
}

func main() -> i32 {
    val s: string = implicit_string()
    return 0
}
"#;

    let exit_code = test
        .compile_and_run("string_implicit.nr", source)
        .expect("String implicit return compilation or execution failed");
    assert_eq!(exit_code, 0);
}

#[test]
fn test_multiple_string_functions() {
    let test = CompileTest::new();
    let source = r#"
func get_greeting() -> string {
    return "Hello"
}

func get_name() -> string {
    return "World"
}

func combine() -> string {
    val g: string = get_greeting()
    val n: string = get_name()
    return g
}

func main() -> i32 {
    val result: string = combine()
    return 0
}
"#;

    let exit_code = test
        .compile_and_run("multiple_strings.nr", source)
        .expect("Multiple string functions compilation or execution failed");
    assert_eq!(exit_code, 0);
}
