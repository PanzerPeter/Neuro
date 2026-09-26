// Function call tests: parameters, nested calls, and function composition
use crate::compile_harness::CompileTest;

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

/// A plain function name is an ordinary value of its function type: it may be passed to
/// a function-typed parameter, bound, and called through the binding. It was refused
/// everywhere but the `|>` target and the `>>` operands.
#[test]
fn test_bug_071_a_function_name_is_a_value() {
    let test = CompileTest::new();
    let source = r#"
func apply_twice(f: (i32) -> i32, x: i32) -> i32 { f(f(x)) }
func inc(x: i32) -> i32 { x + 3 }
func add(a: i32, b: i32) -> i32 { a + b }
func fold(f: (i32, i32) -> i32, a: i32, b: i32) -> i32 { f(a, b) }

func main() -> i32 {
    val g = inc
    apply_twice(inc, 10) + g(1) + fold(add, 2, 5)
}
"#;
    let exit = test
        .compile_and_run("fn_as_value.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 16 + 4 + 7);
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
        err.contains("is a generic function"),
        "expected the generic-function-as-value diagnostic, got: {err}"
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
