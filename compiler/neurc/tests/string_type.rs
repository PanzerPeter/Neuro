// String type tests (Phase 1)
// Tests string literals, parameters, and string variable handling
mod common;
use common::CompileTest;

#[test]
fn test_string_literal_return() {
    let test = CompileTest::new();
    let source = r#"
func get_message() -> string {
    return "Hello, NEURO!"
}

func main() -> i32 {
    val msg: string = get_message()
    return 0
}
"#;

    let exit_code = test
        .compile_and_run("string_literal.nr", source)
        .expect("String literal compilation or execution failed");
    assert_eq!(exit_code, 0);
}

#[test]
fn test_string_parameter() {
    let test = CompileTest::new();
    let source = r#"
func echo(msg: string) -> string {
    return msg
}

func main() -> i32 {
    val result: string = echo("test message")
    return 0
}
"#;

    let exit_code = test
        .compile_and_run("string_parameter.nr", source)
        .expect("String parameter compilation or execution failed");
    assert_eq!(exit_code, 0);
}

#[test]
fn test_string_with_escapes() {
    let test = CompileTest::new();
    let source = r#"
func get_escaped() -> string {
    return "Line1\nLine2\tTabbed"
}

func main() -> i32 {
    val s: string = get_escaped()
    return 0
}
"#;

    let exit_code = test
        .compile_and_run("string_escapes.nr", source)
        .expect("String with escapes compilation or execution failed");
    assert_eq!(exit_code, 0);
}

#[test]
fn test_empty_string() {
    let test = CompileTest::new();
    let source = r#"
func get_empty() -> string {
    return ""
}

func main() -> i32 {
    val empty: string = get_empty()
    return 0
}
"#;

    let exit_code = test
        .compile_and_run("empty_string.nr", source)
        .expect("Empty string compilation or execution failed");
    assert_eq!(exit_code, 0);
}

#[test]
fn test_string_variable_assignment() {
    let test = CompileTest::new();
    let source = r#"
func test_vars() -> string {
    val msg1: string = "First"
    val msg2: string = "Second"
    val msg3: string = msg1
    return msg3
}

func main() -> i32 {
    val result: string = test_vars()
    return 0
}
"#;

    let exit_code = test
        .compile_and_run("string_variables.nr", source)
        .expect("String variable assignment compilation or execution failed");
    assert_eq!(exit_code, 0);
}

#[test]
fn test_string_implicit_return() {
    let test = CompileTest::new();
    let source = r#"
func implicit_string() -> string {
    "Implicit return"
}

func main() -> i32 {
    val s: string = implicit_string()
    return 0
}
"#;

    let exit_code = test
        .compile_and_run("string_implicit.nr", source)
        .expect("String implicit return compilation or execution failed");
    assert_eq!(exit_code, 0);
}

#[test]
fn test_multiple_string_functions() {
    let test = CompileTest::new();
    let source = r#"
func get_greeting() -> string {
    return "Hello"
}

func get_name() -> string {
    return "World"
}

func combine() -> string {
    val g: string = get_greeting()
    val n: string = get_name()
    return g
}

func main() -> i32 {
    val result: string = combine()
    return 0
}
"#;

    let exit_code = test
        .compile_and_run("multiple_strings.nr", source)
        .expect("Multiple string functions compilation or execution failed");
    assert_eq!(exit_code, 0);
}
