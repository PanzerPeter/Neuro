// End-to-end tests for the `char` primitive type (§1.2).

mod common;
use common::CompileTest;

#[test]
fn char_literal_cast_to_int_is_code_point() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val c: char = 'A'
    return c as i32
}
"#;
    let exit_code = test
        .compile_and_run("char_cast.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 65);
}

#[test]
fn char_equality_and_ordering() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: char = 'a'
    val b: char = 'b'
    if a < b && a == 'a' && b != a {
        return 7
    }
    return 0
}
"#;
    let exit_code = test
        .compile_and_run("char_cmp.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 7);
}

#[test]
fn char_is_copy_and_round_trips_through_int() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val c: char = 'Z'
    val copy = c
    val code: i32 = copy as i32
    val back: char = code as char
    if back == c {
        return code
    }
    return 0
}
"#;
    let exit_code = test
        .compile_and_run("char_copy.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 90); // 'Z'
}

#[test]
fn escape_and_unicode_char_literals() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val nl: char = '\n'
    val tab: char = '\t'
    val emoji: char = '\u{1F44D}'
    if nl as i32 == 10 && tab as i32 == 9 && emoji as i32 == 128077 {
        return 3
    }
    return 0
}
"#;
    let exit_code = test
        .compile_and_run("char_escape.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 3);
}

#[test]
fn char_as_function_param_and_return() {
    let test = CompileTest::new();
    let source = r#"
func next_letter(c: char) -> char {
    val code: i32 = c as i32
    return (code + 1) as char
}

func main() -> i32 {
    val b: char = next_letter('a')
    return b as i32
}
"#;
    let exit_code = test
        .compile_and_run("char_param.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 98); // 'b'
}

#[test]
fn char_arithmetic_is_rejected() {
    // §1.2: `char` has no arithmetic — compute on the integer code point instead.
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val c: char = 'a' + 1
    return 0
}
"#;
    let result = test.compile_and_run("char_no_arith.nr", source);
    assert!(
        result.is_err(),
        "char arithmetic must be a compile error, got {result:?}"
    );
}
