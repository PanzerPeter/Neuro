// Regression: a function may be called before its definition appears in the file.
//
// Free-function signatures used to be registered by `check_function` itself, which
// runs per body in source order, so any call to a function defined further down was
// `undefined function` — and mutual recursion could not be written at all. Every
// other item kind (structs, enums, traits, constants) was already order-independent,
// and the LLVM backend already pre-declared every signature; only the type checker
// insisted on definition-before-use.
mod common;
use common::CompileTest;

#[test]
fn regression_call_before_definition() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    helper(3)
}

func helper(x: i32) -> i32 {
    x + 1
}
"#;
    let exit = test
        .compile_and_run("forward_plain.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 4);
}

#[test]
fn regression_mutual_recursion() {
    let test = CompileTest::new();
    let source = r#"
func is_even(n: i32) -> bool {
    if n == 0 { true } else { is_odd(n - 1) }
}

func is_odd(n: i32) -> bool {
    if n == 0 { false } else { is_even(n - 1) }
}

func main() -> i32 {
    if is_even(10) { if is_odd(7) { 42 } else { 1 } } else { 2 }
}
"#;
    let exit = test
        .compile_and_run("forward_mutual.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 42);
}

#[test]
fn regression_forward_reference_returning_struct() {
    let test = CompileTest::new();
    let source = r#"
struct P { x: i32 }

func main() -> i32 {
    make(3).x
}

func make(v: i32) -> P {
    P { x: v }
}
"#;
    let exit = test
        .compile_and_run("forward_struct.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 3);
}

#[test]
fn regression_forward_reference_returning_generic_enum() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    match make(3) {
        Result::Ok(v) => v,
        Result::Err(e) => 0 - e
    }
}

func make(x: i32) -> Result<i32, i32> {
    Result::Ok(x)
}
"#;
    let exit = test
        .compile_and_run("forward_generic_enum.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 3);
}

#[test]
fn duplicate_function_definition_is_still_rejected() {
    // The signature pass owns the duplicate check now; it must not go missing.
    let test = CompileTest::new();
    let source = r#"
func dup() -> i32 { 1 }
func dup() -> i32 { 2 }

func main() -> i32 { dup() }
"#;
    let source_path = test.write_source("forward_duplicate.nr", source);
    assert!(
        test.compile(&source_path).is_err(),
        "a duplicate function definition must not compile"
    );
}
