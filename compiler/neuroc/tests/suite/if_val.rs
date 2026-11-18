// End-to-end tests for `if val PATTERN = value { }` and `while val PATTERN = value { }`:
// the branch taken only on a match, the value an `if val` with `else` carries, chains that
// mix `else if val` with `else if`, the loop that re-tests its scrutinee each pass, and the
// rules both keep from `if` and `while`.

use crate::compile_harness::CompileTest;

#[test]
fn if_val_runs_its_block_only_when_the_pattern_matches() {
    let test = CompileTest::new();
    // The `bump(x)` tail is an `i32` the statement discards, as an `if` without `else`
    // discards its block's value.
    let source = r#"
func bump(n: i32) -> i32 { n + 1 }

func main() -> i32 {
    mut total = 0
    val hit = Some(20)
    val miss: Option<i32> = None
    if val Some(x) = hit {
        total += x
        bump(x)
    }
    if val Some(x) = miss { total += 100 }
    if val None = miss { total += 1 }
    total
}
"#;
    let exit = test
        .compile_and_run("if_val_statement.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 21);
}

#[test]
fn if_val_with_else_is_a_value() {
    let test = CompileTest::new();
    let source = r#"
enum Shape { Circle(i32), Square(i32), Dot }

func side(s: Shape) -> i32 {
    if val Shape::Square(w) = s { w } else { 0 }
}

func main() -> i32 {
    val o = Some(4)
    val doubled = if val Some(x) = o { x * 2 } else { 0 }
    doubled + side(Shape::Square(5)) + side(Shape::Dot) + side(Shape::Circle(9))
}
"#;
    let exit = test
        .compile_and_run("if_val_value.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 13);
}

#[test]
fn chains_mix_else_if_val_with_else_if() {
    let test = CompileTest::new();
    let source = r#"
func pick(flag: bool, a: Option<i32>, b: Option<i32>) -> i32 {
    if flag { 1 }
    else if val Some(x) = a { 10 + x }
    else if val Some(y) = b { 20 + y }
    else if !flag { 3 }
    else { 4 }
}

func main() -> i32 {
    val none: Option<i32> = None
    mut seen = 0
    if false { seen += 100 } else if val Some(x) = Some(7) { seen += x }
    pick(true, Some(5), none) + pick(false, Some(5), none) + pick(false, none, Some(6))
        + pick(false, none, none) + seen
}
"#;
    let exit = test
        .compile_and_run("if_val_chain.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 1 + 15 + 26 + 3 + 7);
}

#[test]
fn while_val_retests_its_scrutinee_each_pass() {
    let test = CompileTest::new();
    // `continue` re-runs `pop()`, and the loop stops at the first `None`.
    let source = r#"
func main() -> i32 {
    mut stack: Vec<i32> = Vec::new()
    stack.push(1)
    stack.push(2)
    stack.push(3)
    stack.push(4)
    mut odd_sum = 0
    mut passes = 0
    while val Some(top) = stack.pop() {
        passes += 1
        if top % 2 == 0 { continue }
        odd_sum += top
    }
    odd_sum * 10 + passes
}
"#;
    let exit = test
        .compile_and_run("while_val_drain.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 44);
}

#[test]
fn while_val_takes_a_label_and_moves_owning_payloads() {
    let test = CompileTest::new();
    let source = r#"
func take(s: StringBuilder) -> u64 { s.len() }

func main() -> i32 {
    mut words: Vec<StringBuilder> = Vec::new()
    mut i = 0
    while i < 3 {
        mut b = StringBuilder::new()
        b.push_str("abcd")
        words.push(b)
        i += 1
    }
    mut total: u64 = 0
    'drain: while val Some(w) = words.pop() {
        mut j = 0
        while j < 10 {
            if total >= 8 { break 'drain }
            j += 1
        }
        total += take(w)
    }
    total as i32
}
"#;
    let exit = test
        .compile_and_run("while_val_label.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 8);
}

#[test]
fn if_val_bindings_do_not_outlive_their_branch() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    if val Some(x) = Some(1) { }
    x
}
"#;
    let message = test
        .check("if_val_scope.nr", source)
        .expect_err("`x` is bound only inside the `if val` block");
    assert!(
        message.contains("undefined variable 'x'"),
        "diagnostic should name the out-of-scope binding; got: {message}"
    );
}

#[test]
fn an_if_val_without_else_carries_no_value() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val z = if val Some(x) = Some(1) { x }
    0
}
"#;
    let message = test
        .check("if_val_no_else_value.nr", source)
        .expect_err("an `if val` with no `else` is unit, like an `if`");
    assert!(
        message.contains("not a value"),
        "diagnostic should say the `if val` has no value; got: {message}"
    );
}

#[test]
fn a_while_val_break_cannot_carry_a_value() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut v: Vec<i32> = Vec::new()
    while val Some(x) = v.pop() { break x }
    0
}
"#;
    let message = test
        .check("while_val_break_value.nr", source)
        .expect_err("`while val` is a `while`, so it yields unit");
    assert!(
        message.contains("only allowed in a 'loop'"),
        "diagnostic should apply the `while` rule; got: {message}"
    );
}

#[test]
fn an_if_val_pattern_must_fit_the_scrutinee() {
    let test = CompileTest::new();
    let source = r#"
enum Light { Red, Green }

func main() -> i32 {
    if val Light::Red = Some(1) { return 1 }
    0
}
"#;
    test.check("if_val_wrong_enum.nr", source)
        .expect_err("a `Light` pattern cannot match an `Option<i32>`");
}

#[test]
fn an_if_val_head_needs_its_equals_sign() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val o = Some(1)
    if val Some(x) o { return x }
    0
}
"#;
    let message = test
        .check("if_val_missing_equal.nr", source)
        .expect_err("`if val PATTERN scrutinee` is missing its `=`");
    assert!(
        message.contains("'=' after an `if val` pattern"),
        "diagnostic should name the missing `=`; got: {message}"
    );
}
