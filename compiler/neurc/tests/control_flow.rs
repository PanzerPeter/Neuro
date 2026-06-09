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

#[test]
fn test_while_loop_accumulation() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut i: i32 = 0
    mut sum: i32 = 0

    while i < 5 {
        sum = sum + i
        i = i + 1
    }

    return sum
}
"#;

    let exit_code = test
        .compile_and_run("while_loop.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 10, "Expected exit code 10 (0+1+2+3+4)");
}

#[test]
fn test_while_loop_with_break() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut i: i32 = 0

    while true {
        if i == 4 {
            break
        }
        i = i + 1
    }

    return i
}
"#;

    let exit_code = test
        .compile_and_run("while_break.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 4, "Expected exit code 4");
}

#[test]
fn test_while_loop_with_continue() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut i: i32 = 0
    mut sum: i32 = 0

    while i < 5 {
        i = i + 1
        if i == 3 {
            continue
        }
        sum = sum + i
    }

    return sum
}
"#;

    let exit_code = test
        .compile_and_run("while_continue.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 12, "Expected exit code 12 (1+2+4+5)");
}

#[test]
fn test_loop_with_break() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut i: i32 = 0

    loop {
        if i == 4 {
            break
        }
        i = i + 1
    }

    return i
}
"#;

    let exit_code = test
        .compile_and_run("loop_break.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 4, "Expected exit code 4");
}

#[test]
fn test_loop_with_continue() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut i: i32 = 0
    mut sum: i32 = 0

    loop {
        i = i + 1
        if i == 3 {
            continue
        }
        if i > 5 {
            break
        }
        sum = sum + i
    }

    return sum
}
"#;

    let exit_code = test
        .compile_and_run("loop_continue.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 12, "Expected exit code 12 (1+2+4+5)");
}

#[test]
fn test_for_range_loop_accumulation() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut sum: i32 = 0

    for i in 0..5 {
        sum = sum + i
    }

    return sum
}
"#;

    let exit_code = test
        .compile_and_run("for_range_sum.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 10, "Expected exit code 10 (0+1+2+3+4)");
}

#[test]
fn test_for_range_loop_with_continue() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut sum: i32 = 0

    for i in 0..6 {
        if i == 3 {
            continue
        }
        sum = sum + i
    }

    return sum
}
"#;

    let exit_code = test
        .compile_and_run("for_range_continue.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 12, "Expected exit code 12 (0+1+2+4+5)");
}

#[test]
fn test_else_if_bare_identifier_condition() {
    // A bare identifier as an `else if` condition previously caused the parser to
    // consume the block-opening `{` as a struct literal, corrupting the parse tree.
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val x: i32 = 5
    val is_big: bool = false
    val is_medium: bool = true
    mut result: i32 = 0

    if x > 10 {
        result = 3
    } else if is_big {
        result = 2
    } else if is_medium {
        result = 1
    } else {
        result = 0
    }

    return result
}
"#;

    let exit_code = test
        .compile_and_run("else_if_identifier.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 1, "Expected exit code 1 (is_medium branch)");
}

#[test]
fn regression_if_else_all_branches_return_no_missing_return_error() {
    // An if/else where every branch has an explicit `return` previously triggered
    // a false "missing return" codegen error because the dead merge block had no
    // terminator. The fix emits `unreachable` for the dead merge block.
    let test = CompileTest::new();
    let source = r#"
func abs(x: i32) -> i32 {
    if x >= 0 {
        return x
    } else {
        return -x
    }
}

func main() -> i32 {
    return abs(-5)
}
"#;
    let exit_code = test
        .compile_and_run("all_branches_return.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 5, "Expected exit code 5 (abs of -5)");
}

#[test]
fn regression_else_if_all_branches_return() {
    // Variant with else-if chain — all arms return explicitly.
    let test = CompileTest::new();
    let source = r#"
func classify(x: i32) -> i32 {
    if x < 0 {
        return -1
    } else if x == 0 {
        return 0
    } else {
        return 1
    }
}

func main() -> i32 {
    return classify(42)
}
"#;
    let exit_code = test
        .compile_and_run("else_if_all_return.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 1, "Expected exit code 1 (positive)");
}
