// End-to-end tests for string interpolation and its format mini-language.
//
// Every case compiles a program that compares a rendered literal against the
// expected text and reports the first mismatch as its exit code, so a failure
// names the specifier that broke rather than just "not zero".

mod common;
use common::CompileTest;

/// A `check` helper plus a `main` that runs `cases` and returns the tag of the
/// first mismatching one, or 0 when they all render as expected.
fn program(bindings: &str, cases: &[(&str, &str)]) -> String {
    let mut source = String::from(
        "func check(actual: string, expected: string, tag: i32) -> i32 {\n    \
         if actual == expected { return 0 }\n    return tag\n}\n\n\
         func main() -> i32 {\n",
    );
    source.push_str(bindings);
    source.push_str("    mut bad: i32 = 0\n");
    for (index, (rendered, expected)) in cases.iter().enumerate() {
        source.push_str(&format!(
            "    if bad == 0 {{ bad = check(\"{}\", \"{}\", {}) }}\n",
            rendered,
            expected,
            index + 1
        ));
    }
    source.push_str("    return bad\n}\n");
    source
}

fn run(name: &str, bindings: &str, cases: &[(&str, &str)]) {
    let test = CompileTest::new();
    let exit_code = test
        .compile_and_run(name, &program(bindings, cases))
        .expect("compilation or execution failed");
    assert_eq!(exit_code, 0, "case {} rendered unexpectedly", exit_code);
}

#[test]
fn holes_render_bindings_and_expressions() {
    run(
        "interp_basic.nr",
        "    val name = \"Neuro\"\n    val a: i32 = 2\n    val b: i32 = 5\n",
        &[
            ("Hello, {name}!", "Hello, Neuro!"),
            ("{a} + {b} = {a + b}", "2 + 5 = 7"),
            ("{a}{b}", "25"),
            ("no holes here", "no holes here"),
            ("{a * b - 1}", "9"),
        ],
    );
}

#[test]
fn escaped_brace_and_bare_close_stay_literal() {
    run(
        "interp_escapes.nr",
        "    val n: i32 = 1\n",
        &[
            ("\\{not a hole}", "\\{not a hole}"),
            ("a}b", "a}b"),
            ("{n}\\{n}", "1\\{n}"),
        ],
    );
}

#[test]
fn float_specifiers_match_the_spec_table() {
    run(
        "interp_floats.nr",
        "    val pi: f64 = 3.14159\n    val whole: f64 = 2.0\n    val tiny: f64 = 0.001\n",
        &[
            ("{pi:.2}", "3.14"),
            ("{pi:.3}", "3.142"),
            ("{pi:e}", "3.14159e0"),
            ("{pi:.2e}", "3.14e0"),
            ("{tiny:e}", "1e-3"),
            ("{whole}", "2.0"),
            ("{pi}", "3.14159"),
        ],
    );
}

#[test]
fn integer_radix_specifiers_match_the_spec_table() {
    run(
        "interp_radix.nr",
        "    val n: i32 = 255\n    val neg: i32 = -1\n",
        &[
            ("{n:d}", "255"),
            ("{n:x}", "ff"),
            ("{n:X}", "FF"),
            ("{n:b}", "11111111"),
            ("{n:o}", "377"),
            // Radix rendering shows the value's own bits, so a negative `i32`
            // renders as its 32-bit two's complement, not a sign-extended `i64`.
            ("{neg:x}", "ffffffff"),
            ("{neg:b}", "11111111111111111111111111111111"),
        ],
    );
}

#[test]
fn width_alignment_and_flags_pad_the_field() {
    run(
        "interp_padding.nr",
        "    val n: i32 = 255\n    val neg: i32 = -42\n    val s = \"hi\"\n",
        &[
            ("{n:8d}", "     255"),
            ("{s:<6}", "hi    "),
            ("{s:>6}", "    hi"),
            ("{s:^6}", "  hi  "),
            ("{s:^5}", " hi  "),
            ("{n:08d}", "00000255"),
            ("{n:+d}", "+255"),
            ("{neg:+d}", "-42"),
            // Zero fill goes after the sign, never in front of it.
            ("{neg:06d}", "-00042"),
            // A value already wider than the field is never truncated.
            ("{n:2d}", "255"),
        ],
    );
}

#[test]
fn non_numeric_scalars_render_and_debug_quote() {
    run(
        "interp_scalars.nr",
        "    val s = \"hi\"\n    val c: char = 'A'\n    val emoji: char = '\\u{1F600}'\n    val yes = true\n    val no = false\n",
        &[
            ("{s}", "hi"),
            ("{s:?}", "\\\"hi\\\""),
            ("{c}", "A"),
            ("{c:?}", "'A'"),
            ("{emoji}", "\\u{1F600}"),
            ("{yes} {no}", "true false"),
        ],
    );
}

#[test]
fn holes_accept_calls_fields_and_nested_blocks() {
    let test = CompileTest::new();
    let source = r#"
struct Point { x: i32, y: i32 }

func double(n: i32) -> i32 {
    return n * 2
}

func main() -> i32 {
    val p = Point { x: 3, y: 4 }
    val a: i32 = 2
    // A hole re-parses as a full expression, so struct-literal colons, call
    // parentheses, and block braces all nest inside it.
    val text = "{p.x},{p.y} {double(a)} {if a < 3 { 1 } else { 0 }} {Point { x: 9, y: 0 }.x} {'}'}"
    if text == "3,4 4 1 9 }" {
        return 0
    }
    return 1
}
"#;
    let exit_code = test
        .compile_and_run("interp_nested.nr", source)
        .expect("compilation or execution failed");
    assert_eq!(exit_code, 0);
}

#[test]
fn interpolation_result_is_a_string_value() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val n: i32 = 7
    val label: string = "n={n}"
    val joined = label + "!"
    return joined.len() as i32
}
"#;
    let exit_code = test
        .compile_and_run("interp_value.nr", source)
        .expect("compilation or execution failed");
    assert_eq!(exit_code, 4);
}

#[test]
fn unterminated_hole_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val n: i32 = 1
    val bad = "value: {n"
    return 0
}
"#;
    let error = test
        .compile_and_run("interp_unterminated.nr", source)
        .expect_err("an unterminated hole must not compile");
    assert!(
        error.contains("never closed"),
        "expected an unterminated-hole diagnostic, got: {error}"
    );
}

#[test]
fn specifier_that_no_type_can_satisfy_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val s = "text"
    val bad = "{s:x}"
    return 0
}
"#;
    let error = test
        .compile_and_run("interp_bad_spec.nr", source)
        .expect_err("hex formatting of a string must not compile");
    assert!(
        error.contains("radix formatting applies to integers"),
        "expected a spec/type mismatch diagnostic, got: {error}"
    );
}

#[test]
fn precision_on_an_integer_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val n: i32 = 1
    val bad = "{n:.2}"
    return 0
}
"#;
    let error = test
        .compile_and_run("interp_int_precision.nr", source)
        .expect_err("fixed-point formatting of an integer must not compile");
    assert!(
        error.contains("apply to floats"),
        "expected a spec/type mismatch diagnostic, got: {error}"
    );
}

#[test]
fn interpolation_is_rejected_in_a_pattern() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val n: i32 = 1
    val s = "a"
    match s {
        "{n}" => return 1,
        _ => return 0,
    }
}
"#;
    let error = test
        .compile_and_run("interp_pattern.nr", source)
        .expect_err("an interpolated literal is not a constant pattern");
    assert!(
        error.contains("not allowed in a pattern"),
        "expected a pattern diagnostic, got: {error}"
    );
}
