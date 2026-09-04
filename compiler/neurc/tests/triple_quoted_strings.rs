// End-to-end tests for triple-quoted (block) string literals.
//
// Each case compiles a program whose `main` compares a block string against the
// text it should dedent to and returns 0 only on an exact match, so a failure
// means the value reaching codegen differed — not merely that the source lexed.

mod common;
use common::CompileTest;

/// Compile a `main` that returns 0 when `block` evaluates to `expected`.
fn assert_block_equals(name: &str, block: &str, expected: &str) {
    let source = format!(
        "func main() -> i32 {{\n    val actual = {block}\n    \
         val expected = \"{expected}\"\n    \
         if actual == expected {{ return 0 }}\n    return 1\n}}\n"
    );
    let test = CompileTest::new();
    let exit_code = test
        .compile_and_run(name, &source)
        .expect("compilation or execution failed");
    assert_eq!(exit_code, 0, "block string did not dedent to {expected:?}");
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
fn block_string_dedents_to_the_closing_delimiter() {
    assert_block_equals(
        "block_dedent.nr",
        "\"\"\"\n        first\n        second\n        \"\"\"",
        "first\\nsecond",
    );
}

#[test]
fn block_string_keeps_indentation_beyond_the_delimiter() {
    assert_block_equals(
        "block_nested_indent.nr",
        "\"\"\"\n        root\n            child\n        \"\"\"",
        "root\\n    child",
    );
}

#[test]
fn block_string_blank_line_normalizes_to_empty() {
    assert_block_equals(
        "block_blank_line.nr",
        "\"\"\"\n        one\n\n        two\n        \"\"\"",
        "one\\n\\ntwo",
    );
}

#[test]
fn block_string_carries_interior_quotes_unescaped() {
    assert_block_equals(
        "block_quotes.nr",
        "\"\"\"\n        he said \"yes\"\n        \"\"\"",
        "he said \\\"yes\\\"",
    );
}

#[test]
fn block_string_interpolates_with_format_specs() {
    let source = "func main() -> i32 {\n    \
                  val name = \"Neuro\"\n    val ratio: f64 = 0.5\n    \
                  val actual = \"\"\"\n        {name} {ratio:.2}\n        \"\"\"\n    \
                  if actual == \"Neuro 0.50\" { return 0 }\n    return 1\n}\n";
    let test = CompileTest::new();
    let exit_code = test
        .compile_and_run("block_interp.nr", source)
        .expect("compilation or execution failed");
    assert_eq!(exit_code, 0, "interpolated block string rendered wrongly");
}

/// The newline before the closing delimiter's line belongs to the delimiter. A block
/// written on one content line is that line and nothing else, all the way through
/// codegen — the value the program compares is the value the lexer built.
#[test]
fn regression_block_string_drops_the_newline_before_its_closing_line() {
    assert_block_equals(
        "block_no_trailing_newline.nr",
        "\"\"\"\n        ab\n        \"\"\"",
        "ab",
    );
}

/// A trailing newline is written by leaving a blank line before the closing line.
#[test]
fn block_string_trailing_newline_is_written_as_a_blank_line() {
    assert_block_equals(
        "block_blank_before_close.nr",
        "\"\"\"\n        ab\n\n        \"\"\"",
        "ab\\n",
    );
}

#[test]
fn under_indented_line_is_rejected() {
    let message = compile_error(
        "block_under_indented.nr",
        "func main() -> i32 {\n    val s = \"\"\"\n        deep\n    shallow\n        \"\"\"\n    return s.len() as i32\n}\n",
    );
    assert!(
        message.contains("indented less than the closing"),
        "unhelpful diagnostic: {message}"
    );
}

#[test]
fn closing_delimiter_sharing_a_line_is_rejected() {
    let message = compile_error(
        "block_trailing_close.nr",
        "func main() -> i32 {\n    val s = \"\"\"\n        text \"\"\"\n    return s.len() as i32\n}\n",
    );
    assert!(
        message.contains("must be on its own line"),
        "unhelpful diagnostic: {message}"
    );
}

#[test]
fn unterminated_block_string_is_rejected() {
    let message = compile_error(
        "block_unterminated.nr",
        "func main() -> i32 {\n    val s = \"\"\"\n        never closed\n    return 0\n}\n",
    );
    assert!(
        message.contains("unterminated triple-quoted string"),
        "unhelpful diagnostic: {message}"
    );
}
