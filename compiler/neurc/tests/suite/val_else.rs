// End-to-end tests for `val PATTERN = value else |binding| { ... }`: the
// binding surviving into the rest of the block, the type-directed `else |name|`, the
// three ways to leave the scope, and the diagnostics for the two ways to get it wrong.
//
// `Option` and `Result` come from the prelude here (unlike the slice-level unit tests,
// which declare their own), so these programs exercise the shipped surface exactly as
// a user writes it.

use crate::compile_harness::CompileTest;

#[test]
fn val_else_unwraps_a_result_and_forwards_the_error() {
    let test = CompileTest::new();
    // `doubled` is bound for the remainder of `handle`, not just one arm: the whole
    // point of the construct over a `match`.
    let source = r#"
func parse(n: i32) -> Result<i32, i32> {
    if n > 0 {
        Result::Ok(n * 2)
    } else {
        Result::Err(7)
    }
}

func handle(n: i32) -> i32 {
    val Result::Ok(doubled) = parse(n) else |err| { return err }
    val bumped = doubled + 1
    bumped
}

func main() -> i32 {
    handle(5) + handle(-1)
}
"#;
    let exit = test
        .compile_and_run("val_else_result.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 18);
}

#[test]
fn val_else_unwraps_an_option_without_a_binding() {
    let test = CompileTest::new();
    let source = r#"
func lookup(key: i32) -> Option<i32> {
    if key == 1 {
        Option::Some(30)
    } else {
        Option::None
    }
}

func first(key: i32) -> i32 {
    val Option::Some(value) = lookup(key) else { return 12 }
    value
}

func main() -> i32 {
    first(1) + first(2)
}
"#;
    let exit = test
        .compile_and_run("val_else_option.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 42);
}

#[test]
fn the_else_branch_may_break_out_of_a_loop() {
    let test = CompileTest::new();
    // The canonical drain loop from the spec: `else { break }` ends the iteration
    // when the source runs dry.
    let source = r#"
func next(i: i32, limit: i32) -> Option<i32> {
    if i < limit {
        Option::Some(i + 1)
    } else {
        Option::None
    }
}

func main() -> i32 {
    mut total: i32 = 0
    mut i: i32 = 0
    loop {
        val Option::Some(v) = next(i, 4) else { break }
        total = total + v
        i = i + 1
    }
    total
}
"#;
    let exit = test
        .compile_and_run("val_else_break.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 10);
}

#[test]
fn a_plain_enum_binds_the_whole_scrutinee_for_a_nested_match() {
    let test = CompileTest::new();
    // Neither Option nor Result, so there is no single "other" variant to unpack:
    // `|s|` names the original value and the else branch discriminates further.
    let source = r#"
enum Shape {
    Circle { radius: i32 },
    Square(i32),
    Empty
}

func area(s: Shape) -> i32 {
    val Shape::Circle { radius } = s else |other| {
        match other {
            Shape::Square(side) => { return side * side },
            _ => { return 0 }
        }
    }
    radius * 3
}

func main() -> i32 {
    area(Shape::Circle { radius: 4 }) + area(Shape::Square(5)) + area(Shape::Empty)
}
"#;
    let exit = test
        .compile_and_run("val_else_enum.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 37);
}

#[test]
fn the_else_branch_may_panic() {
    let test = CompileTest::new();
    // `panic` ends the scope just as `return` does, and the success path is the only
    // one that reaches the binding, so a present value still exits cleanly.
    let source = r#"
func present() -> Option<i32> {
    Option::Some(9)
}

func main() -> i32 {
    val Option::Some(v) = present() else |_| { panic("absent") }
    v
}
"#;
    let exit = test
        .compile_and_run("val_else_panic.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 9);
}

#[test]
fn a_falling_through_else_branch_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
func lookup() -> Option<i32> {
    Option::Some(1)
}

func main() -> i32 {
    val Option::Some(v) = lookup() else { val fallback = 0 }
    v
}
"#;
    let source_path = test.write_source("val_else_falls_through.nr", source);
    let message = test
        .compile(&source_path)
        .expect_err("an `else` branch that can fall through should be a type error");
    assert!(
        message.contains("can fall through"),
        "diagnostic should name the divergence rule; got: {message}"
    );
}

#[test]
fn a_named_else_binding_on_an_option_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
func lookup() -> Option<i32> {
    Option::Some(1)
}

func main() -> i32 {
    val Option::Some(v) = lookup() else |e| { return 0 }
    v
}
"#;
    let source_path = test.write_source("val_else_option_binding.nr", source);
    let message = test
        .compile(&source_path)
        .expect_err("`Option::None` has no payload to bind");
    assert!(
        message.contains("has nothing to bind"),
        "diagnostic should explain the empty variant; got: {message}"
    );
}

#[test]
fn val_else_takes_an_unqualified_prelude_variant() {
    let test = CompileTest::new();
    // The prelude makes `Some(v)` the idiomatic spelling in value position, `match`
    // arms, and here: the qualified form must not be the only one that parses.
    let source = r#"
func half(n: i32) -> Option<i32> {
    if n % 2 == 0 {
        Some(n / 2)
    } else {
        None
    }
}

func main() -> i32 {
    val Some(v) = half(24) else { return 1 }
    v + 1
}
"#;
    let exit = test
        .compile_and_run("val_else_unqualified.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 13);
}

#[test]
fn val_else_takes_an_unqualified_variant_with_an_else_binding() {
    let test = CompileTest::new();
    let source = r#"
func parse(n: i32) -> Result<i32, i32> {
    if n > 0 {
        Ok(n * 2)
    } else {
        Err(7)
    }
}

func handle(n: i32) -> i32 {
    val Ok(doubled) = parse(n) else |err| { return err }
    doubled + 1
}

func main() -> i32 {
    handle(5) + handle(-1)
}
"#;
    let exit = test
        .compile_and_run("val_else_unqualified_binding.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 18);
}

/// A temporary scrutinee has no binding left to own its payload, so the success bindings
/// do, and a miss destroys the value in the `else` block. A named scrutinee keeps its own.
#[test]
fn regression_bug_142_a_temporary_scrutinee_gives_up_its_payload() {
    let source = r#"
func make(i: i32) -> Option<Tensor<f32, [2]>> {
    val t: Tensor<f32, [2]> = [1.0, 2.0]
    if i < 100 { return Some(t) }
    return None
}

func load(i: i32) -> Result<Vec<i32>, Vec<i32>> {
    mut v: Vec<i32> = Vec::new()
    v.push(i)
    if i % 2 == 0 { return Ok(v) }
    return Err(v)
}

func bound(i: i32) -> i32 {
    val Ok(c) = load(i) else |e| { return e.len() as i32 + 10 }
    c.len() as i32
}

func unbound(i: i32) -> i32 {
    val Ok(c) = load(i) else { return 20 }
    c.len() as i32
}

func named(i: i32) -> i32 {
    val r = load(i)
    val Ok(c) = r else { return 30 }
    c.len() as i32
}

func main() -> i32 {
    mut total = 0.0f32
    mut n = 0
    for i in 0..10 {
        val Some(c) = make(i) else { return 2 }
        total += c.sum()
        n += bound(i) + unbound(i) + named(i)
    }
    if total != 30.0 { return 1 }
    n - 300
}
"#;
    let test = CompileTest::new();
    assert_eq!(test.run_leak_checked("val_else_temporary.nr", source), 20);
}

/// A `val ... else` binding is a `val`, so it may shadow a name of its block, the
/// scrutinee's included. It used to be refused as already defined.
#[test]
fn regression_bug_145_a_val_else_binding_shadows_like_any_val() {
    let source = r#"
func wrap(n: i32) -> Option<i32> { Some(n + 1) }

func main() -> i32 {
    val x = 5
    val Some(x) = wrap(x) else { return 1 }
    val Some(x) = wrap(x) else { return 2 }
    val o = wrap(x)
    val Some(o) = o else { return 3 }
    x + o
}
"#;
    let test = CompileTest::new();
    assert_eq!(test.compile_and_run("val_else_shadow.nr", source), Ok(15));
    let duplicate = "enum E { P(i32, i32), Q }\nfunc main() -> i32 {\n    val E::P(a, a) = E::P(1, 2) else { return 1 }\n    a\n}\n";
    let err = test
        .check("val_dup.nr", duplicate)
        .expect_err("a pattern bound a name twice");
    assert!(err.contains("already defined"), "{err}");
}

/// An array pattern is refutable over a slice, so `val [..] = s else` reads as a `val-else`.
#[test]
fn regression_lim_249_an_array_pattern_takes_an_else() {
    let source = r#"
func first(s: &[i32]) -> i32 {
    val [a, ..] = s else { return 7 }
    a
}

func two(s: &[i32]) -> i32 {
    val [a, b, ..] = s else { return 8 }
    a + b
}

func main() -> i32 {
    val none: [i32; 0] = []
    val pair = [4, 5]
    val [p, q, r] = [1, 2, 3]
    first(&pair) + first(&none) + two(&pair) + two(&none) + p + q + r
}
"#;
    let test = CompileTest::new();
    assert_eq!(test.compile_and_run("val_else_array.nr", source), Ok(34));
}
