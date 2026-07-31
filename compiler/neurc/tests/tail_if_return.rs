// Regression: a tail-position `if/else` used as a function's implicit return value.
// The parser represents a statement-position `if` as `Stmt::If`, so the backend's
// implicit-return lowering must recognise a trailing `Stmt::If` (with an `else`)
// and yield its value — not fall through with `unreachable` (which segfaulted).
mod common;
use common::CompileTest;

#[test]
fn tail_if_as_implicit_return() {
    let test = CompileTest::new();
    let source = r#"
func relu(x: i32) -> i32 {
    if x > 0 { x } else { 0 }
}

func main() -> i32 {
    relu(7)
}
"#;
    let exit = test
        .compile_and_run("tail_if_relu.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 7);
}

#[test]
fn tail_if_takes_else_branch() {
    let test = CompileTest::new();
    let source = r#"
func relu(x: i32) -> i32 {
    if x > 0 { x } else { 0 }
}

func main() -> i32 {
    relu(0 - 5)
}
"#;
    let exit = test
        .compile_and_run("tail_if_relu_else.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 0);
}

#[test]
fn tail_if_with_preceding_statements() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val x: i32 = 7
    if x > 0 { x } else { 0 }
}
"#;
    let exit = test
        .compile_and_run("tail_if_main.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 7);
}

#[test]
fn tail_if_elif_chain() {
    let test = CompileTest::new();
    let source = r#"
func classify(n: i32) -> i32 {
    if n < 0 { 0 - 1 } else if n == 0 { 0 } else { 1 }
}

func main() -> i32 {
    classify(42)
}
"#;
    let exit = test
        .compile_and_run("tail_if_elif.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 1);
}

#[test]
fn tail_if_recursive() {
    let test = CompileTest::new();
    let source = r#"
func gcd(a: i32, b: i32) -> i32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

func main() -> i32 {
    gcd(48, 36)
}
"#;
    let exit = test
        .compile_and_run("tail_if_gcd.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 12);
}

#[test]
fn tail_if_in_method() {
    let test = CompileTest::new();
    let source = r#"
struct Counter {
    value: i32,
}

impl Counter {
    func new(v: i32) -> Counter {
        Counter { value: v }
    }

    func sign(&self) -> i32 {
        if self.value > 0 { 1 } else { 0 - 1 }
    }
}

func main() -> i32 {
    val c = Counter::new(9)
    c.sign()
}
"#;
    let exit = test
        .compile_and_run("tail_if_method.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 1);
}

#[test]
fn tail_if_with_explicit_returns_still_works() {
    // The documented workaround must keep compiling and running correctly after the fix.
    let test = CompileTest::new();
    let source = r#"
func relu(x: i32) -> i32 {
    if x > 0 { return x } else { return 0 }
}

func main() -> i32 {
    relu(4)
}
"#;
    let exit = test
        .compile_and_run("tail_if_explicit_return.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 4);
}

// Regression: the same rule one level down. A trailing `if/else` inside another
// block — an if-branch, a bare block, or the tail of a nested block — is that
// block's value too. Recognising it only at the function-body tail meant a nested
// tail `if` lowered as a statement and the enclosing block yielded garbage.

#[test]
fn regression_nested_tail_if_yields_its_value() {
    let test = CompileTest::new();
    let source = r#"
func classify(c: i32) -> i32 {
    if c <= 0 { 7 } else { if c > 50 { 3 } else { c } }
}

func main() -> i32 {
    classify(4)
}
"#;
    let exit = test
        .compile_and_run("nested_tail_if.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 4);
}

#[test]
fn regression_nested_tail_if_in_then_branch() {
    let test = CompileTest::new();
    let source = r#"
func classify(c: i32) -> i32 {
    if c > 0 { if c > 50 { 3 } else { c } } else { 7 }
}

func main() -> i32 {
    classify(4)
}
"#;
    let exit = test
        .compile_and_run("nested_tail_if_then.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 4);
}

#[test]
fn regression_triple_nested_tail_if() {
    let test = CompileTest::new();
    let source = r#"
func classify(c: i32) -> i32 {
    if c <= 0 { 7 } else { if c > 50 { 3 } else { if c > 2 { c } else { 1 } } }
}

func main() -> i32 {
    classify(4)
}
"#;
    let exit = test
        .compile_and_run("triple_nested_tail_if.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 4);
}

#[test]
fn regression_nested_tail_if_after_statements() {
    let test = CompileTest::new();
    let source = r#"
func classify(c: i32) -> i32 {
    if c <= 0 { 7 } else {
        val z = c
        if c > 50 { 3 } else { z }
    }
}

func main() -> i32 {
    classify(4)
}
"#;
    let exit = test
        .compile_and_run("nested_tail_if_stmts.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 4);
}

#[test]
fn regression_nested_tail_if_in_bare_block() {
    let test = CompileTest::new();
    let source = r#"
func classify(c: i32) -> i32 {
    if c <= 0 { 7 } else { { if c > 50 { 3 } else { c } } }
}

func main() -> i32 {
    classify(4)
}
"#;
    let exit = test
        .compile_and_run("nested_tail_if_block.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 4);
}

#[test]
fn regression_nested_tail_if_bound_to_val() {
    // The checker typed such an `if` as `void`, so the binding was a type error.
    let test = CompileTest::new();
    let source = r#"
func classify(c: i32) -> i32 {
    val r = if c <= 0 { 7 } else { if c > 50 { 3 } else { c } }
    r
}

func main() -> i32 {
    classify(4)
}
"#;
    let exit = test
        .compile_and_run("nested_tail_if_val.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 4);
}

#[test]
fn regression_nested_tail_if_agrees_with_elif_chain() {
    // Equivalence class: a nested `if/else` and the flat `else if` chain that means
    // the same thing must produce the same answer.
    let test = CompileTest::new();
    let source = r#"
func nested(c: i32) -> i32 {
    if c <= 0 { 7 } else { if c > 50 { 3 } else { c } }
}

func flat(c: i32) -> i32 {
    if c <= 0 { 7 } else if c > 50 { 3 } else { c }
}

func main() -> i32 {
    if nested(4) == flat(4) { nested(4) } else { 99 }
}
"#;
    let exit = test
        .compile_and_run("nested_tail_if_equiv.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 4);
}

#[test]
fn regression_nested_tail_if_keeps_generic_enum_payload() {
    // The payload came out zeroed: the construction lowered against the erased
    // `Result` template instead of the monomorphized instance, because the nested
    // branch was not a value position at all.
    let test = CompileTest::new();
    let source = r#"
func priced(cost: i32) -> Result<i32, i32> {
    if cost <= 0 {
        Result::Err(7)
    } else {
        if cost > 50 {
            Result::Err(3)
        } else {
            Result::Ok(cost)
        }
    }
}

func main() -> i32 {
    match priced(4) {
        Result::Ok(v) => 100 + v,
        Result::Err(e) => 200 + e
    }
}
"#;
    let exit = test
        .compile_and_run("nested_tail_if_result.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 104);
}

#[test]
fn regression_nested_tail_if_reaches_err_branch() {
    let test = CompileTest::new();
    let source = r#"
func priced(cost: i32) -> Result<i32, i32> {
    if cost <= 0 {
        Result::Err(7)
    } else {
        if cost > 50 {
            Result::Err(3)
        } else {
            Result::Ok(cost)
        }
    }
}

func main() -> i32 {
    match priced(90) {
        Result::Ok(v) => 100 + v,
        Result::Err(e) => 200 + e
    }
}
"#;
    let exit = test
        .compile_and_run("nested_tail_if_result_err.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 203);
}
