mod common;
use common::CompileTest;

#[test]
fn test_if_expr_simple() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val x: i32 = 5
    val result = if x > 3 { 10 } else { 20 }
    result
}
"#;
    let exit = test
        .compile_and_run("if_expr_simple.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 10);
}

#[test]
fn test_if_expr_false_branch() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val x: i32 = 1
    val result = if x > 3 { 10 } else { 20 }
    result
}
"#;
    let exit = test
        .compile_and_run("if_expr_false_branch.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 20);
}

#[test]
fn test_if_expr_as_return_value() {
    let test = CompileTest::new();
    let source = r#"
func abs(n: i32) -> i32 {
    val result = if n >= 0 { n } else { 0 - n }
    result
}

func main() -> i32 {
    abs(0 - 7)
}
"#;
    let exit = test
        .compile_and_run("if_expr_return.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 7);
}

#[test]
fn test_if_expr_elif_chain() {
    let test = CompileTest::new();
    let source = r#"
func classify(n: i32) -> i32 {
    val result = if n < 0 { 0 - 1 } else if n == 0 { 0 } else { 1 }
    result
}

func main() -> i32 {
    val a = classify(0 - 5)
    val b = classify(0)
    val c = classify(3)
    a + b + c
}
"#;
    let exit = test
        .compile_and_run("if_expr_elif.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 0); // -1 + 0 + 1 = 0
}

#[test]
fn test_block_expr_simple() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val result = {
        val a: i32 = 3
        val b: i32 = 4
        a + b
    }
    result
}
"#;
    let exit = test
        .compile_and_run("block_expr_simple.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 7);
}

#[test]
fn test_block_expr_as_return() {
    let test = CompileTest::new();
    let source = r#"
func compute(x: i32) -> i32 {
    {
        val doubled = x + x
        doubled + 1
    }
}

func main() -> i32 {
    compute(5)
}
"#;
    let exit = test
        .compile_and_run("block_expr_return.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 11);
}

#[test]
fn test_if_expr_in_arithmetic() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val x: i32 = 4
    val base: i32 = 10
    val result = base + if x > 2 { 5 } else { 0 - 5 }
    result
}
"#;
    let exit = test
        .compile_and_run("if_expr_arithmetic.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 15);
}
