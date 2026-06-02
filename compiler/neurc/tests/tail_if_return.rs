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
