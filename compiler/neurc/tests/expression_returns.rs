// Expression-based return tests (Phase 1 Feature)
// Tests implicit returns where the last expression in a function is returned
mod common;
use common::CompileTest;

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
