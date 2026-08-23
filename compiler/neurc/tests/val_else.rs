// End-to-end tests for `val PATTERN = value else |binding| { ... }`: the
// binding surviving into the rest of the block, the type-directed `else |name|`, the
// three ways to leave the scope, and the diagnostics for the two ways to get it wrong.
//
// `Option` and `Result` come from the prelude here (unlike the slice-level unit tests,
// which declare their own), so these programs exercise the shipped surface exactly as
// a user writes it.

mod common;
use common::CompileTest;

#[test]
fn val_else_unwraps_a_result_and_forwards_the_error() {
    let test = CompileTest::new();
    // `doubled` is bound for the remainder of `handle`, not just one arm — the whole
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
    // one that reaches the binding — so a present value still exits cleanly.
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
    // arms, and here — the qualified form must not be the only one that parses.
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
