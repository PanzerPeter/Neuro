// End-to-end tests for the prelude's `Option` / `Result` method surface: `.map`,
// `.map_err`, `.ok`, `.ok_or` and the `is_*` tests, a tuple variant passed as the
// function `.map_err` takes, and `??` with a fallback that diverges instead of producing
// a value.

use crate::compile_harness::CompileTest;
use std::process::Command;

#[test]
fn option_methods_transform_convert_and_test() {
    let test = CompileTest::new();
    let source = r#"
func find(key: i32) -> Option<i32> {
    if key > 0 { Some(key * 10) } else { None }
}

func main() -> i32 {
    val doubled = find(2).map(|v| v * 2) ?? 0
    val missing = find(0).map(|v| v * 2) ?? 1
    val as_result: Result<i32, i32> = find(0).ok_or(5)
    val err = match as_result {
        Ok(_) => 0,
        Err(e) => e
    }
    val present = find(1)
    mut flags = 0
    if present.is_some() { flags = flags + 1 }
    if !present.is_none() { flags = flags + 1 }
    doubled + missing + err + flags + (present ?? 0)
}
"#;
    // 40 + 1 + 5 + 2 + 10
    let exit = test
        .compile_and_run("option_methods.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 58);
}

#[test]
fn result_methods_transform_both_sides_and_drop_the_error() {
    let test = CompileTest::new();
    let source = r#"
func divide(a: i32, b: i32) -> Result<i32, string> {
    if b == 0 { Err("divide by zero") } else { Ok(a / b) }
}

func main() -> i32 {
    val good = divide(84, 2).map(|q| q / 2).ok() ?? 0
    val bad = divide(1, 0).map_err(|msg| msg.len())
    val code = match bad {
        Ok(_) => 0,
        Err(n) => n as i32
    }
    val lost = divide(1, 0).ok() ?? 3
    mut flags = 0
    val r = divide(4, 2)
    if r.is_ok() { flags = flags + 1 }
    if !r.is_err() { flags = flags + 1 }
    good + code + lost + flags + (r ?? 0)
}
"#;
    // 21 + 14 + 3 + 2 + 2
    let exit = test
        .compile_and_run("result_methods.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 42);
}

/// The spec's bridge between two error types: `?` converts nothing, so the error is
/// mapped first, here by naming the tuple variant that wraps it.
#[test]
fn map_err_with_a_variant_constructor_bridges_error_types_for_question_mark() {
    let test = CompileTest::new();
    let source = r#"
enum AppError {
    Io(i32),
    Parse(string)
}

func read(n: i32) -> Result<i32, i32> {
    if n < 0 { Err(0 - n) } else { Ok(n) }
}

func parse(n: i32) -> Result<i32, string> {
    if n > 100 { Err("too big") } else { Ok(n + 1) }
}

func process(n: i32) -> Result<i32, AppError> {
    val raw = read(n).map_err(AppError::Io)?
    val parsed = parse(raw).map_err(AppError::Parse)?
    Ok(parsed)
}

func code(r: Result<i32, AppError>) -> i32 {
    match r {
        Ok(v) => v,
        Err(e) => match e {
            AppError::Io(c) => 100 + c,
            AppError::Parse(m) => 200 + m.len() as i32
        }
    }
}

func main() -> i32 {
    val ok = code(process(9))
    val io = code(process(-7))
    val parse = code(process(500))
    if ok != 10 { return 1 }
    if io != 107 { return 2 }
    if parse != 207 { return 3 }
    0
}
"#;
    let exit = test
        .compile_and_run("map_err_variant.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 0);
}

#[test]
fn coalesce_fallback_may_leave_the_scope() {
    let test = CompileTest::new();
    let source = r#"
func first_or_seven(o: Option<i32>) -> i32 {
    val v = o ?? return 7
    v * 2
}

func main() -> i32 {
    mut total = 0
    for i in 0..5 {
        val o: Option<i32> = if i % 2 == 0 { Some(i) } else { None }
        val v = o ?? continue
        total = total + v
    }
    mut last = 0
    loop {
        val o: Option<i32> = if last < 3 { Some(last) } else { None }
        last = o ?? break
        last = last + 1
    }
    total + first_or_seven(None) + first_or_seven(Some(5)) + last
}
"#;
    // 0 + 2 + 4, 7, 10, 3
    let exit = test
        .compile_and_run("coalesce_exit.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 26);
}

/// `?? panic(..)` is the language's unwrap: it yields the payload when there is one and
/// stops the program with the message when there is not.
#[test]
fn coalesce_with_panic_unwraps_or_aborts() {
    let test = CompileTest::new();
    let present = r#"
func main() -> i32 {
    val o: Option<i32> = Some(9)
    o ?? panic("no value")
}
"#;
    let exit = test
        .compile_and_run("unwrap_present.nr", present)
        .expect("compile/run failed");
    assert_eq!(exit, 9);

    let absent = r#"
func main() -> i32 {
    val r: Result<i32, string> = Err("bad")
    val v = r ?? panic("no value")
    v
}
"#;
    let path = test.write_source("unwrap_absent.nr", absent);
    let exe = test.compile(&path).expect("compile failed");
    let output = Command::new(&exe).output().expect("run executable");
    assert!(!output.status.success(), "the absent case must abort");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no value"), "{stderr}");
}

/// `.map` consumes its receiver and `.is_some` only looks at it.
#[test]
fn map_consumes_its_receiver_and_is_some_borrows_it() {
    let test = CompileTest::new();
    let borrowed = r#"
func main() -> i32 {
    val s: Option<string> = Some("abc")
    val seen = s.is_some()
    val n = s.map(|t| t.len()) ?? 0
    if seen { n as i32 } else { 0 }
}
"#;
    let exit = test
        .compile_and_run("is_some_borrows.nr", borrowed)
        .expect("compile/run failed");
    assert_eq!(exit, 3);

    let moved = r#"
func main() -> i32 {
    val s: Option<string> = Some("abc")
    val n = s.map(|t| t.len())
    if s.is_some() { 1 } else { 0 }
}
"#;
    let err = test
        .check("map_moves.nr", moved)
        .expect_err("a value `.map` consumed is used again");
    assert!(err.contains("use of moved value 's'"), "{err}");
}

/// A closure owns a value moved into one of its parameters, as a function does, and
/// releases it when the call returns. `.map` hands its payload to the closure this way.
#[test]
fn closure_releases_an_owned_parameter() {
    let test = CompileTest::new();
    let source = r#"
struct Probe { sink: &mut i32 }

impl Drop for Probe {
    func drop(&mut self) { *self.sink = *self.sink + 1 }
}

func main() -> i32 {
    mut direct: i32 = 0
    {
        val f = |p: Probe| 1
        val r = f(Probe { sink: &mut direct })
    }
    mut mapped: i32 = 0
    {
        val o = Some(Probe { sink: &mut mapped }).map(|p| 2)
    }
    direct * 10 + mapped
}
"#;
    let exit = test
        .compile_and_run("closure_param_drop.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 11);
}

#[test]
fn methods_are_checked_against_the_enum_they_belong_to() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val o: Option<i32> = Some(1)
    val a = o.map_err(|e| e)
    val b = o.ok()
    0
}
"#;
    let err = test
        .check("wrong_enum.nr", source)
        .expect_err("`Option` has no `map_err` or `ok`");
    assert!(err.contains("no method 'map_err'"), "{err}");
    assert!(err.contains("no method 'ok'"), "{err}");
}

/// A generic enum's tuple variant is a function value wherever a function type is
/// expected of it: the expected return names the instance, or the parameters a call has
/// already fixed do (`o.map(Option::Some)` with `o: Option<i32>`).
#[test]
fn generic_variant_is_a_function_value_where_a_function_type_is_expected() {
    let test = CompileTest::new();
    let source = r#"
enum Wrap<T> { One(T), Two(T, T) }
func apply(f: (i32, i32) -> Wrap<i32>) -> i32 {
    match f(2, 3) {
        Wrap::One(v) => v,
        Wrap::Two(a, b) => a * b,
    }
}
func make() -> (i32) -> Option<i32> { Option::Some }
func main() -> i32 {
    val o: Option<i32> = Some(4)
    val nested = o.map(Option::Some)
    val f: (i32) -> Option<i32> = Option::Some
    val r: Result<i32, string> = Result::Ok(5)
    val wrapped = r.map(Option::Some)
    val from_result = match wrapped {
        Result::Ok(inner) => inner ?? 0,
        Result::Err(_) => 0,
    }
    ((nested ?? None) ?? 0) + (f(10) ?? 0) + apply(Wrap::Two) + (make()(20) ?? 0) + from_result
}
"#;
    let exit = test
        .compile_and_run("generic_variant_value.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 4 + 10 + 6 + 20 + 5);
}

#[test]
fn generic_variant_with_no_expected_function_type_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val f = Option::Some
    0
}
"#;
    let err = test
        .check("generic_variant_bare.nr", source)
        .expect_err("nothing names the instance");
    assert!(err.contains("function value only where"), "{err}");
}

/// A program that brings its own `Result` keeps `Option`'s methods; only `.ok_or`, which
/// builds the prelude's `Result`, goes with it.
#[test]
fn shadowing_result_keeps_the_option_methods() {
    let test = CompileTest::new();
    let source = r#"
enum Result { Fine }

func main() -> i32 {
    val o: Option<i32> = Some(20)
    val r = Result::Fine
    o.map(|v| v + 1) ?? 0
}
"#;
    let exit = test
        .compile_and_run("shadow_result.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 21);
}

/// A tuple variant is a function value everywhere one is accepted, so it pipes and
/// composes like a named function.
#[test]
fn variant_constructor_pipes_and_composes() {
    let test = CompileTest::new();
    let source = r#"
enum Wrapped {
    Num(i32)
}

func unwrap(w: Wrapped) -> i32 {
    match w {
        Wrapped::Num(n) => n
    }
}

func main() -> i32 {
    val round_trip = Wrapped::Num >> unwrap
    val piped = 5 |> Wrapped::Num
    round_trip(4) + unwrap(piped)
}
"#;
    let exit = test
        .compile_and_run("variant_pipe.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 9);
}
