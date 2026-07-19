// Integration tests for integer literal type suffixes
mod common;
use common::CompileTest;

#[test]
fn suffix_i64_infers_without_annotation() {
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "suffix_i64.nr",
            r#"
func main() -> i32 {
    val x = 42i64
    val y = 1000000000000i64
    return 0
}
"#,
        )
        .expect("compilation failed");
    assert_eq!(exit, 0);
}

#[test]
fn suffix_u8_infers_without_annotation() {
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "suffix_u8.nr",
            r#"
func main() -> i32 {
    val x = 255u8
    val y = 0u8
    return 0
}
"#,
        )
        .expect("compilation failed");
    assert_eq!(exit, 0);
}

#[test]
fn suffix_consistent_with_explicit_annotation() {
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "suffix_annotated.nr",
            r#"
func main() -> i32 {
    val x: i64 = 42i64
    val y: u32 = 100u32
    return 0
}
"#,
        )
        .expect("compilation failed");
    assert_eq!(exit, 0);
}

#[test]
fn suffix_hex_and_binary() {
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "suffix_other_bases.nr",
            r#"
func main() -> i32 {
    val a = 0xFFu8
    val b = 0b1111_0000i32
    return 0
}
"#,
        )
        .expect("compilation failed");
    assert_eq!(exit, 0);
}

#[test]
fn suffix_range_violation_rejected() {
    let test = CompileTest::new();
    let source_path = test.write_source(
        "suffix_range_err.nr",
        r#"
func main() -> i32 {
    val x = 300u8
    return 0
}
"#,
    );
    // compile() returns Err when the compiler exits non-zero
    assert!(
        test.compile(&source_path).is_err(),
        "expected range error for 300u8"
    );
}

#[test]
fn all_suffix_variants_accepted() {
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "suffix_all.nr",
            r#"
func main() -> i32 {
    val a = 1i8
    val b = 1i16
    val c = 1i32
    val d = 1i64
    val e = 1u8
    val f = 1u16
    val g = 1u32
    val h = 1u64
    return 0
}
"#,
        )
        .expect("compilation failed");
    assert_eq!(exit, 0);
}
