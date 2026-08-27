// End-to-end tests for nesting block comments.
//
// A lexer unit test proves the token stream is right; these prove the *program*
// is right, which is the thing that actually breaks when nesting is wrong. If
// the first `*/` closed the outer comment, the remaining body would reach the
// parser as garbage and compilation would fail outright.

mod common;
use common::CompileTest;

/// Compile and run `source`, asserting it exits with `expected_exit`.
fn assert_exit(name: &str, source: &str, expected_exit: i32) {
    let test = CompileTest::new();
    let exit_code = test
        .compile_and_run(name, source)
        .expect("compilation or execution failed");
    assert_eq!(exit_code, expected_exit, "{name} returned the wrong value");
}

/// Compile a program expected to be rejected, returning the diagnostic text.
fn compile_error(name: &str, source: &str) -> String {
    let test = CompileTest::new();
    let path = test.write_source(name, source);
    match test.compile(&path) {
        Ok(_) => panic!("{name} compiled but should have been rejected"),
        Err(message) => message,
    }
}

#[test]
fn a_nested_comment_is_skipped_whole() {
    assert_exit(
        "nested_comment_skipped.nr",
        "func main() -> i32 {\n\
         \x20   /* outer /* inner */ still outer */\n\
         \x20   return 7\n\
         }\n",
        7,
    );
}

#[test]
fn a_nested_comment_may_span_lines_and_wrap_code() {
    assert_exit(
        "nested_comment_multiline.nr",
        "func main() -> i32 {\n\
         \x20   mut total: i32 = 0\n\
         /* disabled while the loop below is being tuned\n\
         \x20   /* an earlier attempt, kept for reference\n\
         \x20      total = total + 100\n\
         \x20   */\n\
         \x20   total = total + 50\n\
         */\n\
         \x20   for i in 0..5 {\n\
         \x20       total = total + i\n\
         \x20   }\n\
         \x20   return total\n\
         }\n",
        10,
    );
}

/// Deep nesting is the case a depth counter gets wrong when it saturates.
#[test]
fn deeply_nested_comments_close_in_order() {
    assert_exit(
        "nested_comment_deep.nr",
        "func main() -> i32 {\n\
         \x20   /* a /* b /* c /* d */ c */ b */ a */\n\
         \x20   return 3\n\
         }\n",
        3,
    );
}

/// A comment closer inside a string literal must not end a comment that is not
/// open, and a comment opener inside a string must not start one.
#[test]
fn comment_delimiters_inside_string_literals_are_inert() {
    assert_exit(
        "nested_comment_in_string.nr",
        "func main() -> i32 {\n\
         \x20   val s = \"/* not a comment */\"\n\
         \x20   return s.len() as i32\n\
         }\n",
        19,
    );
}

/// One `*/` cannot close two `/*`. The file ends inside a comment, so `main`
/// never exists and the error must name the comment, not the missing function.
#[test]
fn an_unclosed_inner_comment_is_reported() {
    let message = compile_error(
        "nested_comment_unclosed.nr",
        "func main() -> i32 {\n\
         \x20   /* outer /* inner */\n\
         \x20   return 0\n\
         }\n",
    );
    assert!(
        message.contains("block comment"),
        "diagnostic did not mention the block comment: {message}"
    );
}
