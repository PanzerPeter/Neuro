// Variable assignment tests: mutations and reassignments
mod common;
use common::CompileTest;

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
