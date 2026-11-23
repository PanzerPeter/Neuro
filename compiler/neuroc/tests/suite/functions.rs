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

#[test]
fn test_bug_084_main_must_be_declared_without_parameters_returning_i32() {
    // The C runtime reads `main`'s return register as the exit status and passes
    // `argc` as its first argument, so any other signature exits with garbage.
    for main in [
        "func main() {\n    println(\"hi\")\n}",
        "func main() -> bool {\n    true\n}",
        "func main() -> string {\n    \"x\"\n}",
        "func main(x: i32) -> i32 {\n    x\n}",
        "func main<T>() -> i32 {\n    0\n}",
    ] {
        let error = CompileTest::new()
            .check("bad_main.nr", main)
            .expect_err("a `main` with the wrong signature must be refused");
        assert!(
            error.contains("`main` must be declared `func main() -> i32`"),
            "the diagnostic must name the required signature for:\n{main}\ngot: {error}"
        );
    }
}

/// A program function named like a C library function stays the program's own. Before
/// BUG-164 the root module's functions took their bare names as symbols, so `y.exp()` and
/// `2.0.pow(3.0)` ran the program's `exp` and `pow` instead of the math library's, and the
/// backend's own `malloc`, `free` and `write` resolved to the program's. Each form below
/// is computed by hand: the library values, then the program's functions called directly,
/// through a closure, through a generic and as a call argument.
#[test]
fn regression_bug_164_a_c_library_name_stays_the_programs_own() {
    let source = r#"
func exp(x: f64) -> f64 { 100.0 }
func pow(x: f64, y: f64) -> f64 { 0.0 }
func free(x: i32) -> i32 { x + 1 }
func write(a: i32, b: i32, c: i32) -> i32 { a + b + c }
func malloc(n: u64) -> u64 { n * 2 }

func through<T>(value: T, f: (T) -> T) -> T { f(value) }

func main() -> i32 {
    val y: f64 = 1.0
    if (y.exp() * 1000.0) as i32 != 2718 { return 1 }
    if 2.0.pow(3.0) as i32 != 8 { return 2 }
    if exp(y) as i32 != 100 { return 3 }
    val f = |x: f64| exp(x) + pow(x, x)
    if f(y) as i32 != 100 { return 4 }
    if through(y, |x: f64| exp(x)) as i32 != 100 { return 5 }
    if free(write(1, 2, 3)) != 7 { return 6 }
    val head = "ab"
    val text = head + "cd"
    println(text)
    val doubled = malloc(21)
    println("{doubled}")
    0
}
"#;
    for level in ["0", "2"] {
        let exe =
            crate::compile_harness::compile_tagged(source, &format!("bug_164_O{level}"), level);
        let output = std::process::Command::new(&exe).output().expect("run");
        assert_eq!(
            output.status.code(),
            Some(0),
            "-O{level}: stderr {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n"),
            "abcd\n42\n",
            "-O{level}"
        );
    }
}

/// A program's `abort` is not the one a panic calls: the panic still aborts the process
/// with its message, where BUG-164 returned into the program's function and crashed.
#[cfg(unix)]
#[test]
fn regression_bug_164_a_program_abort_leaves_panics_alone() {
    use std::os::unix::process::ExitStatusExt;
    let source = r#"
func abort() -> i32 { 7 }

func main() -> i32 {
    val v = [1, 2, 3]
    val i = abort() - 2
    v[i]
}
"#;
    let exe = crate::compile_harness::compile_tagged(source, "bug_164_abort", "0");
    let output = std::process::Command::new(&exe).output().expect("run");
    assert_eq!(output.status.signal(), Some(6), "{:?}", output.status);
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("index out of bounds"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
