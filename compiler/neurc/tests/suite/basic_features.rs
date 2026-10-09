// Basic feature tests: simple returns, variables, and arithmetic operations
use crate::compile_harness::CompileTest;

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
    if result == -5 { return 7 }
    return 1
}
"#;

    let exit_code = test
        .compile_and_run("negative.nr", source)
        .expect("Compilation or execution failed");
    // A negative exit status is platform-dependent, so the program compares the value
    // itself and reports the outcome as a small positive code.
    assert_eq!(exit_code, 7, "-10 + 5 must be -5");
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
