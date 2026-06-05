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
fn string_len_counts_utf8_bytes_not_codepoints() {
    let test = CompileTest::new();
    // "héllo": h(1) é(2, U+00E9) l(1) l(1) o(1) = 6 UTF-8 bytes, 5 codepoints.
    // `.len()` is a byte count (§2.7), so it must report 6, not 5.
    let source = r#"
func main() -> i32 {
    val n: u64 = "héllo".len()
    return n as i32
}
"#;

    let exit_code = test
        .compile_and_run("string_len_utf8.nr", source)
        .expect("multibyte string.len() compilation or execution failed");
    assert_eq!(exit_code, 6);
}

#[test]
fn string_len_includes_interior_nul_byte() {
    let test = CompileTest::new();
    // "a\0b" has three content bytes; a consumer that stopped at the NUL terminator
    // would see length 1. `.len()` is authoritative and must report 3, proving that
    // string consumers must not rely on null termination (§2.7 literal/runtime guarantee).
    let source = r#"
func main() -> i32 {
    val s: string = "a\0b"
    val n: u64 = s.len()
    return n as i32
}
"#;

    let exit_code = test
        .compile_and_run("string_len_interior_nul.nr", source)
        .expect("interior-NUL string.len() compilation or execution failed");
    assert_eq!(exit_code, 3);
}

#[test]
fn string_clone_preserves_length() {
    let test = CompileTest::new();
    // A clone is byte-for-byte identical, so its length matches the source's.
    let source = r#"
func main() -> i32 {
    val a: string = "hello"
    val b: string = a.clone()
    val n: u64 = b.len()
    return n as i32
}
"#;

    let exit_code = test
        .compile_and_run("string_clone_len.nr", source)
        .expect("string.clone().len() compilation or execution failed");
    assert_eq!(exit_code, 5);
}

#[test]
fn string_clone_is_equal_to_source() {
    let test = CompileTest::new();
    // The clone compares byte-equal to the original.
    let source = r#"
func main() -> i32 {
    val a: string = "neuro"
    val b: string = a.clone()
    if a == b {
        return 7
    }
    return 0
}
"#;

    let exit_code = test
        .compile_and_run("string_clone_eq.nr", source)
        .expect("string.clone() equality compilation or execution failed");
    assert_eq!(exit_code, 7);
}

#[test]
fn string_clone_chained_with_len() {
    let test = CompileTest::new();
    // `.clone()` returns a `string`, so further builtin methods chain off it.
    let source = r#"
func main() -> i32 {
    val n: u64 = "hi, neuro".clone().len()
    return n as i32
}
"#;

    let exit_code = test
        .compile_and_run("string_clone_chain.nr", source)
        .expect("chained string.clone().len() compilation or execution failed");
    assert_eq!(exit_code, 9);
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
