// Function call tests: parameters, nested calls, and function composition
mod common;
use common::CompileTest;

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
