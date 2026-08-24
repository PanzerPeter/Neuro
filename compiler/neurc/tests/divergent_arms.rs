// A divergent arm must not decide an `if`'s or a `match`'s type.
//
// The same rule covers an arm that leaves the scope by `return`, `break`, or
// `continue`: it never reaches the point where the expression has a value.
//
// `panic` and `unreachable` never return, so they take on whatever type their context
// demands and describe nothing about the expression around them. Both the checker and
// HIR lowering took the FIRST arm's type unconditionally, so putting the divergent arm
// first made the whole expression void: the checker dropped the binding ("undefined
// variable"), and lowering reported "void type cannot be used as a value". Writing the
// same two arms in the other order worked, which is the disagreement these tests pin.

mod common;
use common::CompileTest;

#[test]
fn regression_if_with_a_panicking_first_arm_keeps_the_other_arm_type() {
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "if_panic_first.nr",
            r#"
func f(n: i32) -> i32 {
    val v = if n > 0 { panic("no") } else { 2 }
    v
}

func main() -> i32 { f(0 - 1) }
"#,
        )
        .expect("compile/run failed");
    assert_eq!(exit, 2);
}

#[test]
fn if_with_a_panicking_second_arm_agrees_with_the_first_arm_spelling() {
    // The equivalence class: arm order must not change the answer.
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "if_panic_second.nr",
            r#"
func f(n: i32) -> i32 {
    val v = if n <= 0 { 2 } else { panic("no") }
    v
}

func main() -> i32 { f(0 - 1) }
"#,
        )
        .expect("compile/run failed");
    assert_eq!(exit, 2);
}

#[test]
fn regression_match_with_a_panicking_first_arm_keeps_the_other_arm_type() {
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "match_panic_first.nr",
            r#"
func f(n: i32) -> i32 {
    val v = match n {
        n if n > 0 => panic("no"),
        _          => 2
    }
    v
}

func main() -> i32 { f(0 - 1) }
"#,
        )
        .expect("compile/run failed");
    assert_eq!(exit, 2);
}

#[test]
fn regression_unreachable_first_arm_keeps_the_other_arm_type() {
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "if_unreachable_first.nr",
            r#"
func f(n: i32) -> i32 {
    val v = if n > 0 { unreachable() } else { 2 }
    v
}

func main() -> i32 { f(0 - 1) }
"#,
        )
        .expect("compile/run failed");
    assert_eq!(exit, 2);
}

#[test]
fn a_panicking_arm_still_aborts_when_its_path_is_taken() {
    // `abort()` raises SIGABRT, so the process carries no ordinary exit code — the
    // helper reports -1. What matters is that it did not return the other arm's value.
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "if_panic_taken.nr",
            r#"
func f(n: i32) -> i32 {
    val v = if n > 0 { panic("no") } else { 2 }
    v
}

func main() -> i32 { f(5) }
"#,
        )
        .expect("compile/run failed");
    assert_ne!(
        exit, 2,
        "the panicking arm returned a value instead of aborting"
    );
    assert_ne!(
        exit, 0,
        "the panicking arm exited cleanly instead of aborting"
    );
}

#[test]
fn regression_match_arm_that_returns_does_not_type_its_siblings() {
    // The `match` mirror of the tail-`if` rule above: a `{ return .. }` arm typed as
    // `void` and made every other arm a type error.
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "match_arm_returns.nr",
            r#"
func f(n: i32) -> i32 {
    match n {
        0 => 2,
        _ => { return 1 }
    }
}

func main() -> i32 { f(0) * 10 + f(5) }
"#,
        )
        .expect("compile/run failed");
    assert_eq!(exit, 21);
}

#[test]
fn regression_returning_arm_in_expression_position_types_from_its_sibling() {
    // The same rule where the `if` initializes a binding rather than tailing the body.
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "val_if_mixed_arms.nr",
            r#"
func f(n: i32) -> Option<i32> {
    val v = if n > 0 { return Option::None } else { 2 }
    Option::Some(v)
}

func main() -> i32 {
    match f(0 - 1) {
        Option::Some(v) => v,
        Option::None => 9
    }
}
"#,
        )
        .expect("compile/run failed");
    assert_eq!(exit, 2);
}
