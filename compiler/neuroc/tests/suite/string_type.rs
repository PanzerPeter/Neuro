// String type tests (Phase 1)
// Tests string literals, parameters, and string variable handling
use crate::compile_harness::CompileTest;

#[test]
fn test_string_literal_return() {
    let test = CompileTest::new();
    let source = r#"
func get_message() -> string {
    return "Hello, Neuro!"
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

#[test]
fn test_string_equality_equal() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: string = "hello"
    val b: string = "hello"
    if a == b {
        return 0
    }
    return 1
}
"#;

    let exit_code = test
        .compile_and_run("string_eq_equal.nr", source)
        .expect("String equality (equal) compilation or execution failed");
    assert_eq!(exit_code, 0);
}

#[test]
fn test_string_equality_not_equal() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: string = "hello"
    val b: string = "world"
    if a != b {
        return 0
    }
    return 1
}
"#;

    let exit_code = test
        .compile_and_run("string_eq_not_equal.nr", source)
        .expect("String equality (not-equal) compilation or execution failed");
    assert_eq!(exit_code, 0);
}

#[test]
fn test_string_equality_different_lengths() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val short: string = "hi"
    val longer: string = "hello"
    if short != longer {
        return 0
    }
    return 1
}
"#;

    let exit_code = test
        .compile_and_run("string_eq_diff_len.nr", source)
        .expect("String equality (different lengths) compilation or execution failed");
    assert_eq!(exit_code, 0);
}

#[test]
fn test_string_equality_empty() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: string = ""
    val b: string = ""
    val c: string = "x"
    if a == b {
        if a != c {
            return 0
        }
    }
    return 1
}
"#;

    let exit_code = test
        .compile_and_run("string_eq_empty.nr", source)
        .expect("String equality (empty strings) compilation or execution failed");
    assert_eq!(exit_code, 0);
}

/// Interior NUL is legal content, so every byte after it must reach `.rodata`.
/// The literal global used to be built through a C string and end at the first NUL,
/// while `len` still counted the rest, so reads past it came from beyond the global.
#[test]
fn literal_keeps_bytes_after_interior_nul() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val s = "a\0bc"
    if s.len() != 4 { return 1 }
    if s.slice(2..4) != "bc" { return 2 }
    return 0
}
"#;
    let exit_code = test
        .compile_and_run("interior_nul.nr", source)
        .expect("program should compile and run");
    assert_eq!(exit_code, 0);
}

#[test]
fn interpolation_text_keeps_bytes_after_interior_nul() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val n = 7
    val s = "{n}\0bc"
    if s.len() != 4 { return 1 }
    if s.slice(2..4) != "bc" { return 2 }
    return 0
}
"#;
    let exit_code = test
        .compile_and_run("interior_nul_interp.nr", source)
        .expect("program should compile and run");
    assert_eq!(exit_code, 0);
}

/// A string orders by its bytes: `<`, `>`, `<=` and `>=` are built in, a prefix sorts first, and
/// a non-ASCII byte sorts after every ASCII one. Each operator is tested on both sides of an
/// equal pair, which is where a swapped or negated operand order would show.
#[test]
fn strings_order_by_their_bytes() {
    let source = r#"
func main() -> i32 {
    if !("abc" < "abd") || "abd" < "abc" { return 1 }
    if !("ab" < "abc") || "abc" < "ab" { return 2 }
    if !("b" > "a") || "a" > "b" || "a" > "a" { return 3 }
    if !("a" <= "a") || !("a" <= "b") || "b" <= "a" { return 4 }
    if !("a" >= "a") || !("b" >= "a") || "a" >= "b" { return 5 }
    val accented = "é" + ""
    val z = "z"
    if !(&accented > &z) { return 6 }
    if !("x".lt(&"y")) || !("y".ge(&"y")) { return 7 }
    return 0
}
"#;
    assert_eq!(crate::compile_harness::run("string_order.nr", source), 0);
}

/// `string` meets `Comparable`, so a generic body bounded by it, and an ordered map built from a
/// `Comparable` key parameter, instantiate at `string`.
#[test]
fn a_string_meets_a_comparable_bound() {
    let source = r#"
func larger<K: Comparable>(a: K, b: K) -> K {
    if a > b { a } else { b }
}

func first_key<K: Comparable + PartialEq, V>(a: K, b: K, v: V) -> i32 {
    mut m: BTreeMap<K, V> = BTreeMap::new()
    m.insert(a, v)
    return m.len() as i32
}

func main() -> i32 {
    if larger("pear", "fig") != "pear" { return 1 }
    return first_key("b", "a", 7)
}
"#;
    assert_eq!(
        crate::compile_harness::run("string_comparable.nr", source),
        1
    );
}
