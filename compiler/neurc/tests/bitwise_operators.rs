mod common;
use common::CompileTest;

#[test]
fn test_bitwise_and() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: i32 = 0b1100
    val b: i32 = 0b1010
    return a & b
}
"#;
    let exit_code = test
        .compile_and_run("bitwise_and.nr", source)
        .expect("Compilation or execution failed");
    // 0b1100 & 0b1010 = 0b1000 = 8
    assert_eq!(exit_code, 8);
}

#[test]
fn test_bitwise_or() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: i32 = 0b1100
    val b: i32 = 0b1010
    return a | b
}
"#;
    let exit_code = test
        .compile_and_run("bitwise_or.nr", source)
        .expect("Compilation or execution failed");
    // 0b1100 | 0b1010 = 0b1110 = 14
    assert_eq!(exit_code, 14);
}

#[test]
fn test_bitwise_xor() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: i32 = 0b1100
    val b: i32 = 0b1010
    return a ^ b
}
"#;
    let exit_code = test
        .compile_and_run("bitwise_xor.nr", source)
        .expect("Compilation or execution failed");
    // 0b1100 ^ 0b1010 = 0b0110 = 6
    assert_eq!(exit_code, 6);
}

#[test]
fn test_left_shift() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: i32 = 1
    val b: i32 = 4
    return a << b
}
"#;
    let exit_code = test
        .compile_and_run("left_shift.nr", source)
        .expect("Compilation or execution failed");
    // 1 << 4 = 16
    assert_eq!(exit_code, 16);
}

#[test]
fn test_bitwise_not() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: i32 = 0
    val result: i32 = ~a
    // ~0 == -1 in two's complement; return 0 to signal success
    if result == -1 {
        return 0
    }
    return 1
}
"#;
    let exit_code = test
        .compile_and_run("bitwise_not.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 0);
}

#[test]
fn test_bitwise_precedence() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    // << binds tighter than &: (1 << 2) & 7 = 4 & 7 = 4
    val result: i32 = 1 << 2 & 7
    return result
}
"#;
    let exit_code = test
        .compile_and_run("bitwise_precedence.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 4);
}

#[test]
fn test_bitwise_precedence_or_and() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    // & binds tighter than ^: (3 & 5) ^ 6 = 1 ^ 6 = 7
    val result: i32 = 3 & 5 ^ 6
    return result
}
"#;
    let exit_code = test
        .compile_and_run("bitwise_prec_or_and.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 7);
}

#[test]
fn test_bitwise_ops_on_i64() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: i64 = 255
    val b: i64 = 240
    val result: i64 = a & b
    return result as i32
}
"#;
    let exit_code = test
        .compile_and_run("bitwise_i64.nr", source)
        .expect("Compilation or execution failed");
    // 255 & 240 = 0xF0 = 240
    assert_eq!(exit_code, 240);
}

#[test]
fn test_bitwise_type_error_float_rejected() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: f32 = 1.0
    val b: f32 = 2.0
    return (a & b) as i32
}
"#;
    let source_path = test.write_source("bitwise_float_error.nr", source);
    let result = test.compile(&source_path);
    assert!(result.is_err(), "Expected type error for bitwise op on f32");
}

#[test]
fn test_bitnot_type_error_float_rejected() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: f64 = 1.0
    val result: f64 = ~a
    return 0
}
"#;
    let source_path = test.write_source("bitnot_float_error.nr", source);
    let result = test.compile(&source_path);
    assert!(
        result.is_err(),
        "Expected type error for bitwise NOT on f64"
    );
}
