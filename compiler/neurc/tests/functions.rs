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

// A function name in value position reported "undefined variable", denying a name the
// program declares. Functions are a separate namespace with no coercion to a value, and
// the diagnostic has to say so.

#[test]
fn regression_function_name_as_value_names_the_function() {
    let test = CompileTest::new();
    let source = r#"
func apply_twice(f: (i32) -> i32, x: i32) -> i32 { f(f(x)) }
func inc(x: i32) -> i32 { x + 3 }

func main() -> i32 {
    apply_twice(inc, 10)
}
"#;
    let path = test.write_source("fn_as_value.nr", source);
    let err = test
        .compile(&path)
        .expect_err("a function name in value position must not compile");
    assert!(
        err.contains("is a function, not a value"),
        "expected the function-as-value diagnostic, got: {err}"
    );
    assert!(
        !err.contains("undefined variable"),
        "the misleading undefined-variable diagnostic must be gone, got: {err}"
    );
}

#[test]
fn regression_generic_function_name_as_value_names_the_function() {
    let test = CompileTest::new();
    let source = r#"
func identity<T>(x: T) -> T { x }

func main() -> i32 {
    val f = identity
    0
}
"#;
    let path = test.write_source("generic_fn_as_value.nr", source);
    let err = test
        .compile(&path)
        .expect_err("a generic function name in value position must not compile");
    assert!(
        err.contains("is a function, not a value"),
        "expected the function-as-value diagnostic, got: {err}"
    );
}

#[test]
fn regression_truly_undefined_name_still_reports_undefined_variable() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 { nowhere }
"#;
    let path = test.write_source("undefined_name.nr", source);
    let err = test
        .compile(&path)
        .expect_err("an undeclared name must not compile");
    assert!(
        err.contains("undefined variable"),
        "expected the undefined-variable diagnostic, got: {err}"
    );
}
