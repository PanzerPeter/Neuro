// Control flow tests: if/else, comparisons, and logical operators
mod common;
use common::CompileTest;

#[test]
fn test_if_else_true() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val x: i32 = 10
    val result: i32 = 0
    if x > 5 {
        val result: i32 = 100
        return result
    }
    return 50
}
"#;

    let exit_code = test
        .compile_and_run("if_else_true.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 100, "Expected exit code 100");
}

#[test]
fn test_if_else_false() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val x: i32 = 3
    if x > 5 {
        return 100
    }
    return 50
}
"#;

    let exit_code = test
        .compile_and_run("if_else_false.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 50, "Expected exit code 50");
}

#[test]
fn test_comparison_operators() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: i32 = 10
    val b: i32 = 5

    if a == b {
        return 1
    }
    if a != b {
        return 2
    }
    return 0
}
"#;

    let exit_code = test
        .compile_and_run("comparison.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 2, "Expected exit code 2 (a != b)");
}

#[test]
fn test_logical_operators() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: bool = true
    val b: bool = false

    if a && b {
        return 1
    }
    if a || b {
        return 2
    }
    return 3
}
"#;

    let exit_code = test
        .compile_and_run("logical.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 2, "Expected exit code 2 (a || b)");
}

// Note: Deeply nested if/else chains may fail control flow analysis in Phase 1
// The current implementation has limitations with complex control flow patterns
// Simple if/else works, but deeply nested or complex chains may not be recognized
// as having complete return coverage. This is a known limitation for Phase 2.
