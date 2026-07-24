// End-to-end tests for closures and lambdas (§3.12): closure literals with
// Copy-by-value capture, direct calls, and higher-order functions.
mod common;
use common::CompileTest;

#[test]
fn single_expression_closure() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val square = |x: i32| x * x
    square(9)
}
"#;
    let exit = test
        .compile_and_run("closure_square.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 81);
}

#[test]
fn closure_captures_copy_variable_by_value() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val offset = 10
    val add_off = |n: i32| n + offset
    val result = add_off(7)
    // `offset` remains usable after capture: it was copied, not moved.
    result + offset
}
"#;
    let exit = test
        .compile_and_run("closure_capture.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 27);
}

#[test]
fn higher_order_function_takes_closure() {
    let test = CompileTest::new();
    let source = r#"
func apply(v: i32, f: (i32) -> i32) -> i32 {
    f(v)
}

func main() -> i32 {
    val bump = 3
    apply(5, |x: i32| x + bump)
}
"#;
    let exit = test
        .compile_and_run("closure_higher_order.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 8);
}

#[test]
fn closure_binding_passed_to_higher_order_function() {
    let test = CompileTest::new();
    let source = r#"
func apply(v: i32, f: (i32) -> i32) -> i32 {
    f(v)
}

func main() -> i32 {
    val triple = |x: i32| x * 3
    apply(4, triple)
}
"#;
    let exit = test
        .compile_and_run("closure_pass_binding.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 12);
}

#[test]
fn multi_param_block_body_and_move_capture() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val base = 100
    val combine = move |a: i32, b: i32| -> i32 {
        val s = a + b
        return s + base
    }
    combine(2, 3)
}
"#;
    let exit = test
        .compile_and_run("closure_move_block.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 105);
}

#[test]
fn zero_parameter_closure() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val answer = 42
    val get = || answer
    get()
}
"#;
    let exit = test
        .compile_and_run("closure_zero_param.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 42);
}

#[test]
fn closure_block_body_with_tail_if() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val abs = |x: i32| -> i32 { if x > 0 { x } else { 0 - x } }
    abs(0 - 15)
}
"#;
    let exit = test
        .compile_and_run("closure_tail_if.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 15);
}

#[test]
fn capturing_non_copy_value_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val s: string = "hello"
    val f = |x: i32| -> i32 {
        val _keep = s
        x
    }
    f(1)
}
"#;
    let source_path = test.write_source("closure_noncopy.nr", source);
    let err = test
        .compile(&source_path)
        .expect_err("capturing a non-Copy value should be a type error");
    assert!(
        err.contains("non-Copy"),
        "expected a non-Copy capture diagnostic, got: {err}"
    );
}

#[test]
fn closure_parameter_needs_type_annotation() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val f = |x| x + 1
    f(1)
}
"#;
    let source_path = test.write_source("closure_untyped_param.nr", source);
    let err = test
        .compile(&source_path)
        .expect_err("an unannotated closure parameter should be a type error");
    assert!(
        err.contains("type annotation"),
        "expected a parameter-annotation diagnostic, got: {err}"
    );
}
