// String concatenation tests (Phase 1.7)
// `string + string` allocates a new owned, immutable string on the heap.
// Correctness is verified at runtime through the existing byte-level `==`.
mod common;
use common::CompileTest;

#[test]
fn test_concat_two_literals() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val joined: string = "foo" + "bar"
    if joined == "foobar" {
        return 0
    }
    return 1
}
"#;

    let exit_code = test
        .compile_and_run("concat_literals.nr", source)
        .expect("String concat compilation or execution failed");
    assert_eq!(exit_code, 0);
}

#[test]
fn test_concat_variables() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: string = "hello, "
    val b: string = "world"
    val greeting: string = a + b
    if greeting == "hello, world" {
        return 0
    }
    return 1
}
"#;

    let exit_code = test
        .compile_and_run("concat_variables.nr", source)
        .expect("String concat (variables) compilation or execution failed");
    assert_eq!(exit_code, 0);
}

#[test]
fn test_concat_empty_operands() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: string = "" + "x"
    val b: string = "x" + ""
    val c: string = "" + ""
    if a == "x" {
        if b == "x" {
            if c == "" {
                return 0
            }
        }
    }
    return 1
}
"#;

    let exit_code = test
        .compile_and_run("concat_empty.nr", source)
        .expect("String concat (empty operands) compilation or execution failed");
    assert_eq!(exit_code, 0);
}

#[test]
fn test_concat_with_borrow() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: string = "ab"
    val b: string = "cd"
    val r: &string = &b
    val joined: string = a + r
    if joined == "abcd" {
        return 0
    }
    return 1
}
"#;

    let exit_code = test
        .compile_and_run("concat_borrow.nr", source)
        .expect("String concat (borrowed operand) compilation or execution failed");
    assert_eq!(exit_code, 0);
}

#[test]
fn test_concat_does_not_move_operands() {
    let test = CompileTest::new();
    // `+` reads its operands like `==` does; both remain usable afterward.
    let source = r#"
func main() -> i32 {
    val a: string = "left"
    val b: string = "right"
    val joined: string = a + b
    if a == "left" {
        if b == "right" {
            if joined == "leftright" {
                return 0
            }
        }
    }
    return 1
}
"#;

    let exit_code = test
        .compile_and_run("concat_no_move.nr", source)
        .expect("String concat (no move) compilation or execution failed");
    assert_eq!(exit_code, 0);
}

#[test]
fn test_string_minus_string_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val bad: string = "a" - "b"
    return 0
}
"#;

    // Only `+` joins strings; `-` must fail type-checking.
    let result = test.compile_and_run("concat_bad_minus.nr", source);
    assert!(result.is_err(), "expected `string - string` to be rejected");
}
