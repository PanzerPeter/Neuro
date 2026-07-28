// End-to-end tests for the `??` operator: unwrapping `Option<T>` / `Result<T, E>` with
// an inline fallback, the discarded `Err` payload, right-to-left chaining, the laziness
// of the fallback, and the diagnostics for a non-fallible left operand or a mistyped
// fallback.
//
// `Option` and `Result` come from the prelude here (unlike the slice-level unit tests,
// which declare their own), so these programs exercise the shipped surface exactly as a
// user writes it.

mod common;
use common::CompileTest;

#[test]
fn coalesce_unwraps_an_option_or_supplies_the_fallback() {
    let test = CompileTest::new();
    let source = r#"
func lookup(key: i32) -> Option<i32> {
    if key == 1 {
        Option::Some(30)
    } else {
        Option::None
    }
}

func main() -> i32 {
    val found = lookup(1) ?? 0
    val missing = lookup(2) ?? 12
    found + missing
}
"#;
    let exit = test
        .compile_and_run("coalesce_option.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 42);
}

#[test]
fn coalesce_discards_the_error_payload() {
    let test = CompileTest::new();
    // The `Err` payload is 99; the fallback answers to `T` alone, so it never appears.
    let source = r#"
func divide(a: i32, b: i32) -> Result<i32, i32> {
    if b == 0 {
        Result::Err(99)
    } else {
        Result::Ok(a / b)
    }
}

func main() -> i32 {
    val good = divide(40, 4) ?? 0
    val bad = divide(1, 0) ?? 8
    good + bad
}
"#;
    let exit = test
        .compile_and_run("coalesce_result.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 18);
}

#[test]
fn coalesce_chains_right_to_left() {
    let test = CompileTest::new();
    // `a ?? b ?? c` is `a ?? (b ?? c)`: the first present value wins, and the bare
    // fallback is only reached when every fallible operand before it was absent.
    let source = r#"
func lookup(key: i32) -> Option<i32> {
    if key == 1 {
        Option::Some(5)
    } else {
        Option::None
    }
}

func main() -> i32 {
    val first = lookup(1) ?? lookup(0) ?? 40
    val second = lookup(0) ?? lookup(1) ?? 40
    val neither = lookup(0) ?? lookup(2) ?? 40
    first + second + neither
}
"#;
    let exit = test
        .compile_and_run("coalesce_chain.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 50);
}

#[test]
fn the_fallback_is_lazy() {
    let test = CompileTest::new();
    // The fallback aborts the process. Reaching exit code 7 therefore proves it was
    // never evaluated — laziness is observable, not just documented.
    let source = r#"
func never_runs() -> i32 {
    panic("the fallback of a present value must not be evaluated")
}

func present() -> Option<i32> {
    Option::Some(7)
}

func main() -> i32 {
    present() ?? never_runs()
}
"#;
    let exit = test
        .compile_and_run("coalesce_lazy.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 7);
}

#[test]
fn coalesce_composes_with_a_fallible_builtin() {
    let test = CompileTest::new();
    // `checked_mul` and `Vec::get` both return the prelude `Option<T>`, so `??` reads
    // them without a `match` — the pattern the operator exists for.
    let source = r#"
func main() -> i32 {
    val safe: i32 = 1000
    val product = safe.checked_mul(3) ?? 0
    val overflowed = 2000000000.checked_mul(4) ?? 40

    mut items: Vec<i32> = Vec::new()
    items.push(2)
    val present = items.get(0) ?? 0
    val past_end = items.get(9) ?? 1

    product / 1000 + overflowed + present + past_end
}
"#;
    let exit = test
        .compile_and_run("coalesce_builtins.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 46);
}

#[test]
fn coalesce_on_a_non_fallible_value_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val x: i32 = 5
    val y = x ?? 1
    y
}
"#;
    let source_path = test.write_source("coalesce_non_fallible.nr", source);
    let message = test
        .compile(&source_path)
        .expect_err("`??` on an i32 should be a type error");
    assert!(
        message.contains("`??` expects an `Option<T>` or `Result<T, E>`"),
        "diagnostic should name the requirement; got: {message}"
    );
}

#[test]
fn a_fallback_of_the_wrong_type_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
func maybe() -> Option<i32> {
    Option::Some(1)
}

func main() -> i32 {
    val y = maybe() ?? true
    0
}
"#;
    let source_path = test.write_source("coalesce_bad_fallback.nr", source);
    let message = test
        .compile(&source_path)
        .expect_err("a bool fallback for an i32 payload should be a type error");
    assert!(
        message.contains("expected i32") && message.contains("found bool"),
        "diagnostic should report the payload/fallback mismatch; got: {message}"
    );
}
