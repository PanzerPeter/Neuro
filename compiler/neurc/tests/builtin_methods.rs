// Builtin method dispatch on primitive & string types (Phase 1.5 §2, §2.7)
// End-to-end coverage for the first intrinsic: `string.len()`.
mod common;
use common::CompileTest;

#[test]
fn string_len_returns_byte_length() {
    let test = CompileTest::new();
    // "hello" is 5 ASCII bytes; the program exits with that length.
    let source = r#"
func main() -> i32 {
    val s: string = "hello"
    val n: u64 = s.len()
    return n as i32
}
"#;

    let exit_code = test
        .compile_and_run("string_len_basic.nr", source)
        .expect("string.len() compilation or execution failed");
    assert_eq!(exit_code, 5);
}

#[test]
fn empty_string_len_is_zero() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val s: string = ""
    val n: u64 = s.len()
    return n as i32
}
"#;

    let exit_code = test
        .compile_and_run("string_len_empty.nr", source)
        .expect("empty string.len() compilation or execution failed");
    assert_eq!(exit_code, 0);
}

#[test]
fn string_len_on_literal_receiver() {
    let test = CompileTest::new();
    // Receiver is a string literal rather than a binding.
    let source = r#"
func main() -> i32 {
    val n: u64 = "neuro".len()
    return n as i32
}
"#;

    let exit_code = test
        .compile_and_run("string_len_literal.nr", source)
        .expect("literal string.len() compilation or execution failed");
    assert_eq!(exit_code, 5);
}

#[test]
fn unknown_builtin_method_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val s: string = "hello"
    val n: u64 = s.capacity()
    return 0
}
"#;

    let source_path = test.write_source("string_unknown_method.nr", source);
    let result = test.compile(&source_path);
    assert!(
        result.is_err(),
        "expected compilation to fail for an unknown builtin method"
    );
}
