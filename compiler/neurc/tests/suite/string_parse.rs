// End-to-end tests for `string.parse::<T>()`: text read as an integer, a float or a `bool`,
// answered as `Result<T, ParseError>`.
use crate::compile_harness::CompileTest;
use std::process::Command;

const HELPERS: &str = r#"
func empty<T>(r: Result<T, ParseError>) -> bool {
    match r {
        Err(ParseError::Empty) => true,
        _ => false
    }
}
func invalid<T>(r: Result<T, ParseError>) -> bool {
    match r {
        Err(ParseError::Invalid) => true,
        _ => false
    }
}
func out_of_range<T>(r: Result<T, ParseError>) -> bool {
    match r {
        Err(ParseError::OutOfRange) => true,
        _ => false
    }
}
"#;

/// Run a program whose `main` returns the 1-based position of the first check that fails,
/// or 0 when every check holds.
fn assert_all_hold(checks: &[String]) {
    assert!(checks.len() < 256, "exit codes stop at 255");
    let body: String = checks
        .iter()
        .enumerate()
        .map(|(i, check)| format!("    if !({check}) {{\n        return {}\n    }}\n", i + 1))
        .collect();
    let source = format!("{HELPERS}\nfunc main() -> i32 {{\n{body}    0\n}}\n");
    let exit = CompileTest::new()
        .compile_and_run("checks.nr", &source)
        .expect("the checks compile");
    if exit != 0 {
        panic!("failed: {}", checks[exit as usize - 1]);
    }
}

fn owned(checks: &[&str]) -> Vec<String> {
    checks.iter().map(|check| check.to_string()).collect()
}

/// `text` read as `ty` is `want`.
fn parses_to(text: &str, ty: &str, want: &str) -> String {
    format!("(\"{text}\".parse::<{ty}>() ?? {want} + 1{ty}) == {want}")
}

#[test]
fn every_integer_type_reads_its_exact_range() {
    let mut checks = Vec::new();
    for (ty, min, max, below, above) in [
        ("i8", "-128", "127", "-129", "128"),
        ("i16", "-32768", "32767", "-32769", "32768"),
        (
            "i32",
            "-2147483648",
            "2147483647",
            "-2147483649",
            "2147483648",
        ),
        ("u8", "0", "255", "-1", "256"),
        ("u16", "0", "65535", "-1", "65536"),
        ("u32", "0", "4294967295", "-1", "4294967296"),
    ] {
        checks.push(format!("\"{min}\".parse::<{ty}>().is_ok()"));
        checks.push(format!("\"{max}\".parse::<{ty}>().is_ok()"));
        checks.push(format!("out_of_range(\"{below}\".parse::<{ty}>())"));
        checks.push(format!("out_of_range(\"{above}\".parse::<{ty}>())"));
        if min != "0" {
            checks.push(format!("(\"{min}\".parse::<{ty}>() ?? 0{ty}) == {min}{ty}"));
        }
        checks.push(format!("(\"{max}\".parse::<{ty}>() ?? 0{ty}) == {max}{ty}"));
    }
    assert_all_hold(&checks);
}

#[test]
fn sixty_four_bit_limits_survive_the_accumulator() {
    assert_all_hold(&owned(&[
        r#"("9223372036854775807".parse::<i64>() ?? 0i64) == 9223372036854775807i64"#,
        r#""-9223372036854775808".parse::<i64>().is_ok()"#,
        r#"out_of_range("9223372036854775808".parse::<i64>())"#,
        r#"out_of_range("-9223372036854775809".parse::<i64>())"#,
        r#"("18446744073709551615".parse::<u64>() ?? 0u64) == 18446744073709551615u64"#,
        r#"out_of_range("18446744073709551616".parse::<u64>())"#,
        r#"out_of_range("99999999999999999999999999999999".parse::<u64>())"#,
    ]));
}

#[test]
fn integer_text_is_a_sign_and_decimal_digits() {
    let mut checks = owned(&[
        r#"empty("".parse::<i32>())"#,
        r#"invalid("-".parse::<i32>())"#,
        r#"invalid("+".parse::<i32>())"#,
        r#"invalid("--1".parse::<i32>())"#,
        r#"invalid(" 1".parse::<i32>())"#,
        r#"invalid("1 ".parse::<i32>())"#,
        r#"invalid("1\n".parse::<i32>())"#,
        r#"invalid("1_000".parse::<i32>())"#,
        r#"invalid("0x10".parse::<i32>())"#,
        r#"invalid("1.0".parse::<i32>())"#,
        r#"invalid("1e3".parse::<i32>())"#,
        r#"invalid("١٢".parse::<i32>())"#,
        // A bad byte outranks a value too large.
        r#"invalid("99999999999999999999x".parse::<i32>())"#,
    ]);
    checks.push(parses_to("+7", "i32", "7"));
    checks.push(parses_to("007", "i32", "7"));
    checks.push(parses_to("-0", "i32", "0"));
    // Zero is in range on an unsigned type however it is signed.
    checks.push(parses_to("-0", "u8", "0"));
    assert_all_hold(&checks);
}

#[test]
fn bool_text_is_exactly_true_or_false() {
    assert_all_hold(&owned(&[
        r#""true".parse::<bool>() ?? false"#,
        r#"!("false".parse::<bool>() ?? true)"#,
        r#"empty("".parse::<bool>())"#,
        r#"invalid("True".parse::<bool>())"#,
        r#"invalid("1".parse::<bool>())"#,
        r#"invalid("true ".parse::<bool>())"#,
        r#"invalid("falsey".parse::<bool>())"#,
    ]));
}

#[test]
fn f64_is_correctly_rounded() {
    assert_all_hold(&owned(&[
        r#"("0.1".parse::<f64>() ?? 0.0) == 0.1"#,
        r#"("1e23".parse::<f64>() ?? 0.0) == 1e23"#,
        r#"("-2.5".parse::<f64>() ?? 0.0) == -2.5"#,
        r#"("+2.5E1".parse::<f64>() ?? 0.0) == 25.0"#,
        r#"(".5".parse::<f64>() ?? 0.0) == 0.5"#,
        r#"("5.".parse::<f64>() ?? 0.0) == 5.0"#,
        // 2^53 + 1 is a tie: it rounds to even, and any digit past it rounds up.
        r#"("9007199254740993".parse::<f64>() ?? 0.0) == 9007199254740992.0"#,
        r#"("9007199254740993.000000000000000000000001".parse::<f64>() ?? 0.0) == 9007199254740994.0"#,
        // The smallest subnormal, the midpoint below it, and just past that midpoint.
        r#"("4.9406564584124654e-324".parse::<f64>() ?? 0.0) == 5e-324"#,
        r#"("2.4703282292062327e-324".parse::<f64>() ?? 1.0) == 0.0"#,
        r#"("2.4703282292062328e-324".parse::<f64>() ?? 0.0) == 5e-324"#,
        // The largest finite value, and the midpoint above it.
        r#"("1.7976931348623157e308".parse::<f64>() ?? 0.0) == 1.7976931348623157e308"#,
        r#"("1.797693134862315807e308".parse::<f64>() ?? 0.0) == 1.7976931348623157e308"#,
        r#"("1.797693134862315808e308".parse::<f64>() ?? 0.0) > 1.7976931348623157e308"#,
    ]));
}

/// A digit past the parser's cap still decides a tie, and only a nonzero one does.
#[test]
fn digits_past_the_cap_still_break_a_tie() {
    let tie = "1.00000000000000011102230246251565404236316680908203125";
    let padding = "0".repeat(1000);
    assert_all_hold(&[
        format!("(\"{tie}{padding}\".parse::<f64>() ?? 0.0) == 1.0"),
        format!("(\"{tie}{padding}1\".parse::<f64>() ?? 0.0) > 1.0"),
        format!(
            "(\"{}1\".parse::<f64>() ?? 0.0) > 1.7976931348623157e308",
            "9".repeat(1000)
        ),
    ]);
}

#[test]
fn a_float_past_its_range_rounds_as_ieee_does() {
    assert_all_hold(&owned(&[
        r#"("1e400".parse::<f64>() ?? 0.0) > 1.7976931348623157e308"#,
        r#"("-1e400".parse::<f64>() ?? 0.0) < -1.7976931348623157e308"#,
        r#"("1e-400".parse::<f64>() ?? 1.0) == 0.0"#,
        r#"("1e99999999999999999999".parse::<f64>() ?? 0.0) > 1.7976931348623157e308"#,
        r#"("1e-99999999999999999999".parse::<f64>() ?? 1.0) == 0.0"#,
        r#"("0e99999999999999999999".parse::<f64>() ?? 1.0) == 0.0"#,
        r#"("1e39".parse::<f32>() ?? 0.0f32) > 3.4028235e38f32"#,
    ]));
}

#[test]
fn float_text_covers_the_words_the_language_prints() {
    assert_all_hold(&owned(&[
        r#"("inf".parse::<f64>() ?? 0.0) > 1.7976931348623157e308"#,
        r#"("-inf".parse::<f64>() ?? 0.0) < -1.7976931348623157e308"#,
        r#"("Infinity".parse::<f64>() ?? 0.0) > 1.7976931348623157e308"#,
        r#"("INF".parse::<f32>() ?? 0.0f32) > 3.4028235e38f32"#,
        r#"("nan".parse::<f64>() ?? 0.0).is_nan()"#,
        r#"("-NaN".parse::<f32>() ?? 0.0f32).is_nan()"#,
        r#"("nan".parse::<f16>() ?? 0.0f16).is_nan()"#,
        r#"("nan".parse::<bf16>() ?? 0.0bf16).is_nan()"#,
    ]));
}

#[test]
fn float_text_refuses_everything_else() {
    let mut checks = vec![r#"empty("".parse::<f64>())"#.to_string()];
    for text in [
        ".", "-", "+", "e5", ".e5", "1e", "1e+", "1e-", "1ex", "1.2.3", "--1", " 1", "1 ", "1_0",
        "0x1p3", "infin", "nanny", "1f",
    ] {
        checks.push(format!("invalid(\"{text}\".parse::<f64>())"));
    }
    assert_all_hold(&checks);
}

#[test]
fn f32_rounds_once_from_the_text() {
    assert_all_hold(&owned(&[
        r#"("0.1".parse::<f32>() ?? 0.0f32) == 0.1f32"#,
        // `1 + 2^-24` is a tie in `f32`, and through `f64` the digit past it would be lost.
        r#"("1.000000059604644775390625".parse::<f32>() ?? 0.0f32) == 1.0f32"#,
        r#"("1.00000005960464477539062500000000001".parse::<f32>() ?? 0.0f32) > 1.0f32"#,
        r#"("1e-45".parse::<f32>() ?? 0.0f32) > 0.0f32"#,
        // Half the smallest subnormal is 7.006e-46, so these sit either side of the midpoint.
        r#"("7.1e-46".parse::<f32>() ?? 1.0f32) == 1e-45f32"#,
        r#"("7e-46".parse::<f32>() ?? 1.0f32) == 0.0f32"#,
    ]));
}

/// The trap for a parser that goes through `f64`: the text is past a midpoint of the narrow
/// format by less than `f64` can see, so a double rounding lands on the wrong side of it.
#[test]
fn half_precision_rounds_once_from_the_text() {
    assert_all_hold(&owned(&[
        r#"("1.5".parse::<f16>() ?? 0.0f16) == 1.5f16"#,
        r#"("0.1".parse::<f16>() ?? 0.0f16) == 0.1f16"#,
        // `1 + 2^-11` is a tie in `f16`.
        r#"("1.00048828125".parse::<f16>() ?? 0.0f16) == 1.0f16"#,
        r#"("1.00048828125000000000000001".parse::<f16>() ?? 0.0f16) > 1.0f16"#,
        // `1 + 2^-8` is a tie in `bf16`.
        r#"("1.00390625".parse::<bf16>() ?? 0.0bf16) == 1.0bf16"#,
        r#"("1.00390625000000000000000001".parse::<bf16>() ?? 0.0bf16) > 1.0bf16"#,
        r#"("0.1".parse::<bf16>() ?? 0.0bf16) == 0.1bf16"#,
        // The largest finite `f16`, and the tie above it, which rounds to infinity.
        r#"("65504".parse::<f16>() ?? 0.0f16) == 65504.0f16"#,
        r#"("65519.99".parse::<f16>() ?? 0.0f16) == 65504.0f16"#,
        r#"("65520".parse::<f16>() ?? 0.0f16) > 65504.0f16"#,
        // The smallest `f16` subnormal is 2^-24, and half of it is a tie toward zero.
        r#"("5.9604644775390625e-8".parse::<f16>() ?? 0.0f16) > 0.0f16"#,
        r#"("2.98023223876953125e-8".parse::<f16>() ?? 1.0f16) == 0.0f16"#,
        r#"("2.98023223876953126e-8".parse::<f16>() ?? 0.0f16) > 0.0f16"#,
        // `bf16` shares `f32`'s exponent, so this is far outside `f16` and still finite.
        r#"("1e30".parse::<bf16>() ?? 0.0bf16) == 1e30bf16"#,
    ]));
}

#[test]
fn every_receiver_form_reads_and_keeps_the_text() {
    let source = r#"
struct Config {
    port: string
}

func by_borrow(text: &string) -> i32 {
    text.parse::<i32>() ?? -1
}

func by_mut_borrow(text: &mut string) -> i32 {
    text.parse::<i32>() ?? -1
}

func by_value(text: string) -> i32 {
    text.parse::<i32>() ?? -1
}

func main() -> i32 {
    val text = "21"
    val first = text.parse::<i32>() ?? 0
    val second = text.parse::<i32>() ?? 0
    if text != "21" || first + second != 42 {
        return 1
    }
    if by_borrow(&text) != 21 {
        return 2
    }
    mut writable = "5"
    if by_mut_borrow(&mut writable) != 5 {
        return 3
    }
    if by_value("1" + "1") != 11 {
        return 4
    }
    if (("4" + "2").parse::<i32>() ?? 0) != 42 {
        return 5
    }
    val digits = "x123y"
    if (digits.slice(1..4).parse::<i32>() ?? 0) != 123 {
        return 6
    }
    mut words: Vec<string> = Vec::new()
    words.push("7" + "8")
    if (words[0].parse::<i32>() ?? 0) != 78 {
        return 7
    }
    val config = Config { port: "80" + "80" }
    if (config.port.parse::<i32>() ?? 0) != 8080 {
        return 8
    }
    0
}
"#;
    assert_eq!(
        CompileTest::new().compile_and_run("receivers.nr", source),
        Ok(0)
    );
}

#[test]
fn the_result_composes_with_the_error_handling_surface() {
    let source = r#"
func add(a: &string, b: &string) -> Result<i64, ParseError> {
    val x = a.parse::<i64>()?
    val y = b.parse::<i64>()?
    Ok(x + y)
}

func first_or_zero(text: &string) -> i32 {
    val Ok(n) = text.parse::<i32>() else {
        return 0
    }
    n
}

func main() -> i32 {
    val one = "1"
    val two = "2"
    val bad = "two"
    if (add(&one, &two) ?? 0i64) != 3i64 {
        return 1
    }
    if add(&one, &bad).is_ok() {
        return 2
    }
    if first_or_zero(&two) != 2 || first_or_zero(&bad) != 0 {
        return 3
    }
    val doubled = "21".parse::<i32>().map(|n| n * 2)
    if (doubled ?? 0) != 42 {
        return 4
    }
    if "x".parse::<i32>().ok().is_some() {
        return 5
    }
    if val Ok(n) = "6".parse::<i32>() {
        if n != 6 {
            return 6
        }
    } else {
        return 7
    }
    val code = "9".parse::<i32>().map_err(|e| 1) ?? 0
    if code != 9 {
        return 8
    }
    mut total = 0
    for piece in ["1", "x", "3"] {
        total += piece.parse::<i32>() ?? 0
    }
    if total != 4 {
        return 9
    }
    0
}
"#;
    assert_eq!(
        CompileTest::new().compile_and_run("compose.nr", source),
        Ok(0)
    );
}

#[test]
fn a_parse_in_a_long_loop_keeps_a_flat_stack() {
    let source = r#"
func main() -> i32 {
    mut total: i64 = 0
    mut i = 0
    while i < 3000000 {
        total += "1".parse::<i64>() ?? 0i64
        i += 1
    }
    if total == 3000000i64 { 0 } else { 1 }
}
"#;
    assert_eq!(CompileTest::new().compile_and_run("loop.nr", source), Ok(0));
}

#[test]
fn a_parse_error_renders_through_debug() {
    let test = CompileTest::new();
    let source = test.write_source(
        "debug.nr",
        r#"
func main() -> i32 {
    val empty = "".parse::<i32>()
    val invalid = "x".parse::<i32>()
    val range = "300".parse::<u8>()
    match empty {
        Err(e) => println("{e:?}"),
        Ok(_) => println("no")
    }
    match invalid {
        Err(e) => println("{e:?}"),
        Ok(_) => println("no")
    }
    match range {
        Err(e) => println("{e:?}"),
        Ok(_) => println("no")
    }
    0
}
"#,
    );
    let exe = test.compile(&source).expect("the program compiles");
    let output = Command::new(&exe).output().expect("the program runs");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n"),
        "Empty\nInvalid\nOutOfRange\n"
    );
}

fn refused(source: &str) -> String {
    CompileTest::new()
        .check("refused.nr", source)
        .expect_err("the program is refused")
}

#[test]
fn the_turbofish_is_required_and_names_a_parsable_type() {
    let error = refused("func main() -> i32 {\n    val r = \"1\".parse()\n    0\n}\n");
    assert!(error.contains("'parse' declares 1"), "{error}");

    for ty in ["string", "char", "Point", "Meters", "[i32; 2]"] {
        let error = refused(&format!(
            "struct Point {{ x: i32 }}\nnewtype Meters = i32\nfunc main() -> i32 {{\n    val r = \"1\".parse::<{ty}>()\n    0\n}}\n"
        ));
        assert!(
            error.contains("reads an integer, a float or a `bool`"),
            "{ty}: {error}"
        );
    }
}

#[test]
fn an_alias_of_a_parsable_type_is_parsable() {
    let source = "type Count = i32\nfunc main() -> i32 {\n    \"7\".parse::<Count>() ?? 0\n}\n";
    assert_eq!(
        CompileTest::new().compile_and_run("alias.nr", source),
        Ok(7)
    );
}

#[test]
fn a_type_parameter_is_refused_like_to_checked_s() {
    let error = refused(
        "func read<T>(s: &string) -> Result<T, ParseError> {\n    s.parse::<T>()\n}\nfunc main() -> i32 { 0 }\n",
    );
    assert!(
        error.contains("reads an integer, a float or a `bool`"),
        "{error}"
    );
}

#[test]
fn only_a_string_has_the_method() {
    let error =
        refused("func main() -> i32 {\n    val n = 3\n    val r = n.parse::<i32>()\n    0\n}\n");
    assert!(error.contains("'parse'"), "{error}");
}

#[test]
fn a_user_parse_method_keeps_its_own_meaning() {
    let source = "struct Token { n: i32 }\nimpl Token {\n    func parse(&self) -> i32 { self.n }\n}\nfunc main() -> i32 {\n    val t = Token { n: 3 }\n    t.parse()\n}\n";
    assert_eq!(CompileTest::new().compile_and_run("user.nr", source), Ok(3));
}

#[test]
fn a_program_that_replaces_the_prelude_types_is_told_so() {
    let call = "func main() -> i32 {\n    val r = \"1\".parse::<i32>()\n    0\n}\n";
    for (declared, name) in [
        ("enum ParseError { Bad }", "ParseError"),
        ("struct ParseError { line: i32 }", "ParseError"),
        ("enum Result { Fine }", "Result"),
        ("enum Result<T, E> { Good(T), Bad(E) }", "Result"),
    ] {
        let error = refused(&format!("{declared}\n{call}"));
        assert!(
            error.contains(&format!("replaces or removes `{name}`")),
            "`{declared}`: {error}"
        );
    }
    let error = refused(&format!("@no_prelude\n{call}"));
    assert!(error.contains("replaces or removes"), "{error}");
}

#[test]
fn a_program_with_its_own_parse_error_compiles_while_it_does_not_parse() {
    let source = "enum ParseError { Bad(i32) }\nfunc main() -> i32 {\n    match ParseError::Bad(4) {\n        ParseError::Bad(n) => n\n    }\n}\n";
    assert_eq!(CompileTest::new().compile_and_run("own.nr", source), Ok(4));
}

/// The variants read with the enum's name, so words this plain stay the program's.
#[test]
fn parse_error_variants_are_not_bare_names() {
    let error = refused("func main() -> i32 {\n    val e = Empty\n    0\n}\n");
    assert!(error.contains("undefined variable 'Empty'"), "{error}");

    let source = "struct Empty { a: i32 }\nenum Kind { Invalid, OutOfRange }\nfunc main() -> i32 {\n    val e = Empty { a: 5 }\n    val k = Kind::OutOfRange\n    e.a\n}\n";
    assert_eq!(CompileTest::new().compile_and_run("bare.nr", source), Ok(5));
}
