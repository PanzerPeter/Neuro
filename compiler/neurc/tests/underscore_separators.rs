// Integration tests for underscore digit separators in numeric literals (§1.2)
// Underscores are readability-only and are stripped by the lexer before parsing,
// so a program using them must compile and produce the same value as without.
mod common;
use common::CompileTest;

#[test]
fn decimal_underscores_compile_and_run() {
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "underscore_decimal.nr",
            r#"
func main() -> i32 {
    val million = 1_000_000
    val digits = 1_2_3
    return (million / 1_000) - 877 - (digits - 123)
}
"#,
        )
        .expect("compilation failed");
    // 1_000_000 / 1_000 = 1000; 1000 - 877 = 123; digits term contributes 0.
    assert_eq!(exit, 123);
}

#[test]
fn hex_binary_octal_underscores() {
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "underscore_bases.nr",
            r#"
func main() -> i32 {
    val hex = 0x00_FF      // 255
    val bin = 0b0000_1010  // 10
    val oct = 0o0_17       // 15
    return hex - bin - oct - 230
}
"#,
        )
        .expect("compilation failed");
    // 255 - 10 - 15 - 230 = 0.
    assert_eq!(exit, 0);
}

#[test]
fn float_underscores() {
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "underscore_float.nr",
            r#"
func main() -> i32 {
    val big = 1_000.000_5
    val scaled = 2_0e1   // 200.0
    return (big as i32) - 1000 + (scaled as i32) - 200
}
"#,
        )
        .expect("compilation failed");
    // (1000) - 1000 + (200) - 200 = 0.
    assert_eq!(exit, 0);
}

#[test]
fn suffixed_underscores() {
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "underscore_suffixed.nr",
            r#"
func main() -> i32 {
    val a = 1_000i32
    val b = 0xFF_FFi32   // 65535
    return (b - a) - 64535
}
"#,
        )
        .expect("compilation failed");
    // 65535 - 1000 = 64535; 64535 - 64535 = 0.
    assert_eq!(exit, 0);
}
