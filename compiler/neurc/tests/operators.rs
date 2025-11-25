// Arithmetic operator tests: basic operations, division, modulo, and complex expressions
mod common;
use common::CompileTest;

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

// Note: Float operations are supported but not tested here
// because exit codes are integers. Float support is verified
// by the compiler's type system and codegen tests.
