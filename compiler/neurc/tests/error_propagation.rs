// End-to-end tests for the `?` operator: unwrapping `Result<T, E>` / `Option<T>` or
// leaving the enclosing function with the failure, the error payload surviving the trip
// unchanged, short-circuiting inside loops, and the diagnostics for an operand or an
// enclosing function that cannot carry a failure.
//
// `Option` and `Result` come from the prelude here (unlike the slice-level unit tests,
// which declare their own), so these programs exercise the shipped surface exactly as a
// user writes it.

mod common;
use common::CompileTest;

#[test]
fn try_unwraps_ok_and_propagates_err() {
    let test = CompileTest::new();
    let source = r#"
func halve(n: i32) -> Result<i32, i32> {
    if n % 2 == 0 {
        Result::Ok(n / 2)
    } else {
        Result::Err(n)
    }
}

func quarter(n: i32) -> Result<i32, i32> {
    val half = halve(n)?
    val rest = halve(half)?
    Result::Ok(rest)
}

func main() -> i32 {
    val ok = quarter(40) ?? 0
    val failed = quarter(6) ?? 100
    ok + failed
}
"#;
    let exit = test
        .compile_and_run("try_result.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 110);
}

#[test]
fn the_error_payload_travels_unchanged() {
    let test = CompileTest::new();
    // `?` forwards the original `Err` value — there is no conversion step — so the
    // caller sees the exact payload the callee produced.
    let source = r#"
func reject(n: i32) -> Result<i32, i32> {
    Result::Err(n * 3)
}

func forward(n: i32) -> Result<i32, i32> {
    val v = reject(n)?
    Result::Ok(v)
}

func main() -> i32 {
    match forward(9) {
        Result::Ok(v) => v,
        Result::Err(e) => e
    }
}
"#;
    let exit = test
        .compile_and_run("try_error_identity.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 27);
}

#[test]
fn try_propagates_none_out_of_an_option_function() {
    let test = CompileTest::new();
    let source = r#"
func digit(n: i32) -> Option<i32> {
    if n >= 0 && n <= 9 {
        Option::Some(n)
    } else {
        Option::None
    }
}

func digit_sum(a: i32, b: i32) -> Option<i32> {
    val x = digit(a)?
    val y = digit(b)?
    Option::Some(x + y)
}

func main() -> i32 {
    val summed = digit_sum(4, 5) ?? 0
    val missing = digit_sum(4, 55) ?? 30
    summed + missing
}
"#;
    let exit = test
        .compile_and_run("try_option.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 39);
}

#[test]
fn try_short_circuits_the_rest_of_the_body() {
    let test = CompileTest::new();
    // The statement after a failing `?` must never run: reaching it aborts, so the
    // exit code proves the function left immediately.
    let source = r#"
func fails() -> Result<i32, i32> {
    Result::Err(4)
}

func stops() -> Result<i32, i32> {
    val v = fails()?
    panic("everything after a failing `?` is unreachable")
}

func main() -> i32 {
    stops() ?? 21
}
"#;
    let exit = test
        .compile_and_run("try_short_circuit.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 21);
}

#[test]
fn try_leaves_a_loop_body() {
    let test = CompileTest::new();
    // A `?` inside a loop returns from the whole function, not just the iteration.
    let source = r#"
func check(n: i32) -> Result<i32, i32> {
    if n < 10 {
        Result::Ok(n)
    } else {
        Result::Err(n)
    }
}

func total(limit: i32) -> Result<i32, i32> {
    mut sum: i32 = 0
    for i in 0..limit {
        sum = sum + check(i)?
    }
    Result::Ok(sum)
}

func main() -> i32 {
    val complete = total(5) ?? 0
    val aborted = match total(12) {
        Result::Ok(v) => v,
        Result::Err(e) => e
    }
    complete + aborted
}
"#;
    let exit = test
        .compile_and_run("try_loop.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 20);
}

#[test]
fn try_composes_with_a_fallible_builtin() {
    let test = CompileTest::new();
    // `checked_mul` returns the prelude `Option<T>`, so `?` reads it directly.
    let source = r#"
func scaled(n: i32) -> Option<i32> {
    val product = n.checked_mul(3)?
    Option::Some(product)
}

func main() -> i32 {
    val ok = scaled(7) ?? 0
    val overflowed = scaled(2000000000) ?? 9
    ok + overflowed
}
"#;
    let exit = test
        .compile_and_run("try_builtin.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 30);
}

#[test]
fn try_on_a_non_fallible_value_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val x: i32 = 5
    val y = x?
    0
}
"#;
    let source_path = test.write_source("try_non_fallible.nr", source);
    let message = test
        .compile(&source_path)
        .expect_err("`?` on an i32 should be a type error");
    assert!(
        message.contains("`?` expects an `Option<T>` or `Result<T, E>`"),
        "diagnostic should name the requirement; got: {message}"
    );
}

#[test]
fn try_in_a_non_fallible_function_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
func maybe() -> Option<i32> {
    Option::Some(1)
}

func main() -> i32 {
    val y = maybe()?
    0
}
"#;
    let source_path = test.write_source("try_wrong_return.nr", source);
    let message = test
        .compile(&source_path)
        .expect_err("a function returning i32 cannot carry a failure");
    assert!(
        message.contains("has nowhere to propagate"),
        "diagnostic should explain the return-type requirement; got: {message}"
    );
}

#[test]
fn a_mismatched_error_type_is_rejected() {
    let test = CompileTest::new();
    // No implicit conversion: the callee's `E` must already be the caller's `E`.
    let source = r#"
func fails() -> Result<i32, bool> {
    Result::Err(true)
}

func wrap() -> Result<i32, i32> {
    val v = fails()?
    Result::Ok(v)
}

func main() -> i32 {
    wrap() ?? 0
}
"#;
    let source_path = test.write_source("try_error_mismatch.nr", source);
    let message = test
        .compile(&source_path)
        .expect_err("a bool error cannot propagate into an i32-error function");
    assert!(
        message.contains("expected i32") && message.contains("found bool"),
        "diagnostic should report the error-payload mismatch; got: {message}"
    );
}
