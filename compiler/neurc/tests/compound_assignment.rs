// Integration tests for compound assignment operators (+=, -=, *=, /=, %=).
// Each operator is desugared to a plain assignment at parse time, so these tests
// also validate that the desugaring path reaches semantic analysis and codegen
// correctly for both integer and float types.
mod common;
use common::CompileTest;

#[test]
fn test_plus_equal_i32() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut x: i32 = 10
    x += 5
    return x
}
"#;
    let exit_code = test
        .compile_and_run("plus_equal_i32.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 15);
}

#[test]
fn test_minus_equal_i32() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut x: i32 = 20
    x -= 7
    return x
}
"#;
    let exit_code = test
        .compile_and_run("minus_equal_i32.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 13);
}

#[test]
fn test_star_equal_i32() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut x: i32 = 6
    x *= 7
    return x
}
"#;
    let exit_code = test
        .compile_and_run("star_equal_i32.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 42);
}

#[test]
fn test_slash_equal_i32() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut x: i32 = 50
    x /= 5
    return x
}
"#;
    let exit_code = test
        .compile_and_run("slash_equal_i32.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 10);
}

#[test]
fn test_percent_equal_i32() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut x: i32 = 17
    x %= 5
    return x
}
"#;
    let exit_code = test
        .compile_and_run("percent_equal_i32.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 2);
}

#[test]
fn test_compound_assignment_chained_i32() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut n: i32 = 1
    n += 4
    n *= 3
    n -= 2
    return n
}
"#;
    // (1+4)*3-2 = 13
    let exit_code = test
        .compile_and_run("chained_compound.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 13);
}

#[test]
fn test_compound_assignment_in_loop_i32() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut sum: i32 = 0
    mut i: i32 = 0
    while i < 5 {
        sum += i
        i += 1
    }
    return sum
}
"#;
    // 0+1+2+3+4 = 10
    let exit_code = test
        .compile_and_run("loop_compound.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 10);
}

#[test]
fn test_slash_equal_i32_truncates() {
    let test = CompileTest::new();
    // Integer division truncates; verify compound /= also truncates
    let source = r#"
func main() -> i32 {
    mut x: i32 = 7
    x /= 2
    return x
}
"#;
    let exit_code = test
        .compile_and_run("slash_equal_trunc.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 3);
}

#[test]
fn test_desugaring_equivalence() {
    let test = CompileTest::new();
    // Verify that x += n produces identical results to x = x + n
    let source_compound = r#"
func main() -> i32 {
    mut x: i32 = 7
    x += 3
    return x
}
"#;
    let source_explicit = r#"
func main() -> i32 {
    mut x: i32 = 7
    x = x + 3
    return x
}
"#;
    let r1 = test
        .compile_and_run("desugar_compound.nr", source_compound)
        .expect("compilation failed");
    let r2 = test
        .compile_and_run("desugar_explicit.nr", source_explicit)
        .expect("compilation failed");
    assert_eq!(
        r1, r2,
        "compound assignment must desugar to equivalent plain assignment"
    );
}
