// Binding a `void` initializer must be a type error, not an internal codegen error.
//
// `void` has no value representation, so the backend's value path can only answer it
// with an internal error — the wrong place for the program to be caught. Every shape
// below reaches that path through a different expression form, and each one used to
// pass `neurc check` and abort code generation. The `if` / `match` / block / `loop`
// spellings are the reason the check tests the binding's TYPE rather than whether its
// initializer happens to be a call: only two of these have a callee at all.

mod common;
use common::CompileTest;

/// Compile `source` and return the compiler's combined output, asserting it failed.
fn expect_compile_error(filename: &str, source: &str) -> String {
    let test = CompileTest::new();
    let source_path = test.write_source(filename, source);
    match test.compile(&source_path) {
        Ok(_) => panic!("binding a void initializer should not compile: {filename}"),
        Err(output) => output,
    }
}

/// Every void-initializer spelling must be rejected by the type checker with the same
/// diagnostic, and must never reach the backend.
fn assert_void_binding_diagnostic(filename: &str, source: &str) {
    let output = expect_compile_error(filename, source);
    assert!(
        output.contains("the initializer has type void"),
        "expected the void-binding type error for {filename}, got:\n{output}"
    );
    assert!(
        !output.contains("internal compiler error"),
        "{filename} reached the backend instead of being type-checked:\n{output}"
    );
    assert!(
        !output.contains("void type cannot be used as a value"),
        "{filename} reached the backend instead of being type-checked:\n{output}"
    );
}

#[test]
fn regression_binding_a_void_builtin_call_is_a_type_error() {
    assert_void_binding_diagnostic(
        "void_bind_println.nr",
        r#"
func main() -> i32 {
    val x = println("hi")
    return 0
}
"#,
    );
}

#[test]
fn regression_binding_a_void_user_function_is_a_type_error() {
    assert_void_binding_diagnostic(
        "void_bind_user_func.nr",
        r#"
func nothing() {
    val a = 1
}

func main() -> i32 {
    val x = nothing()
    return 0
}
"#,
    );
}

#[test]
fn regression_a_mut_binding_of_a_void_call_is_a_type_error() {
    assert_void_binding_diagnostic(
        "void_bind_mut.nr",
        r#"
func main() -> i32 {
    mut x = println("hi")
    return 0
}
"#,
    );
}

#[test]
fn regression_binding_an_if_whose_branches_are_void_is_a_type_error() {
    assert_void_binding_diagnostic(
        "void_bind_if.nr",
        r#"
func main() -> i32 {
    val c = true
    val x = if c { println("a") } else { println("b") }
    return 0
}
"#,
    );
}

#[test]
fn regression_binding_a_match_whose_arms_are_void_is_a_type_error() {
    assert_void_binding_diagnostic(
        "void_bind_match.nr",
        r#"
enum Tag { A, B }

func main() -> i32 {
    val t = Tag::A
    val x = match t {
        Tag::A => println("a"),
        Tag::B => println("b")
    }
    return 0
}
"#,
    );
}

#[test]
fn regression_binding_a_block_with_a_void_tail_is_a_type_error() {
    assert_void_binding_diagnostic(
        "void_bind_block.nr",
        r#"
func main() -> i32 {
    val x = { println("a") }
    return 0
}
"#,
    );
}

#[test]
fn regression_binding_a_loop_that_breaks_without_a_value_is_a_type_error() {
    assert_void_binding_diagnostic(
        "void_bind_loop.nr",
        r#"
func main() -> i32 {
    val x = loop { break }
    return 0
}
"#,
    );
}

#[test]
fn regression_an_explicit_void_annotation_is_a_type_error() {
    assert_void_binding_diagnostic(
        "void_bind_annotated.nr",
        r#"
func nothing() {}

func main() -> i32 {
    val x: void = nothing()
    return 0
}
"#,
    );
}

/// The fix must not touch statement position: a `void` call on its own line is the
/// normal way to call one, and an `if` / `match` used as a statement is unaffected.
#[test]
fn a_void_call_in_statement_position_still_compiles() {
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "void_statement_position.nr",
            r#"
func nothing() {
    println("side effect")
}

func main() -> i32 {
    nothing()
    println("direct")
    val n = 3
    if n > 0 { println("pos") } else { println("non-pos") }
    return 7
}
"#,
        )
        .expect("a void call in statement position must still compile and run");
    assert_eq!(exit, 7);
}
