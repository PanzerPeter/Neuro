//! Closure inference and escaping closures: a closure's parameter types come from the
//! function type its context gives it and its return type from its body; a closure passed
//! to a generic higher-order function takes part in inferring the call's type parameters;
//! and a closure that borrows nothing owns its environment, so it may be returned, stored
//! and copied, its environment released with the last copy.

use crate::compile_harness::CompileTest;

fn run(filename: &str, source: &str) -> i32 {
    CompileTest::new()
        .compile_and_run(filename, source)
        .expect("compile/run failed")
}

fn refused(filename: &str, source: &str) -> String {
    CompileTest::new()
        .check(filename, source)
        .expect_err("the program should be refused")
}

/// The program's stdout and exit code.
fn output(filename: &str, source: &str) -> (String, Option<i32>) {
    let test = CompileTest::new();
    let exe = test
        .compile(&test.write_source(filename, source))
        .expect("compile failed");
    let run = std::process::Command::new(&exe)
        .output()
        .expect("run executable");
    (
        String::from_utf8_lossy(&run.stdout).into_owned(),
        run.status.code(),
    )
}

#[test]
fn a_parameter_takes_its_type_from_the_context() {
    let source = r#"
struct Calc { op: (i32, i32) -> i32 }
func twice(f: (i32) -> i32, x: i32) -> i32 { f(f(x)) }
func pick() -> (i32) -> i32 { |x| x * 10 }
func main() -> i32 {
    val k = 3
    val square: (i32) -> i32 = |x| x * x
    mut shift: (i32) -> i32 = |x| x
    shift = |x| x + k
    val calc = Calc { op: |a, b| a - b }
    val op = calc.op
    val ten = pick()
    return twice(|x| x + k, 1) + square(4) + shift(1) + op(9, 4) + ten(1)
}
"#;
    assert_eq!(run("context.nr", source), 7 + 16 + 4 + 5 + 10);
}

#[test]
fn a_block_body_returns_its_last_expression() {
    let source = r#"
func apply(f: (i32) -> i64, x: i32) -> i64 { f(x) }
func main() -> i32 {
    val widen = |x: i32| {
        val doubled = x * 2
        doubled as i64
    }
    val sign = |x: i32| {
        if x < 0 { -1 } else { 1 }
    }
    val wide = apply(|x| {
        val y = x + 1
        y as i64
    }, 4)
    return (widen(20) + wide) as i32 + sign(-5)
}
"#;
    assert_eq!(run("block_body.nr", source), 40 + 5 - 1);
}

#[test]
fn a_tensor_traversal_and_a_for_head_type_their_closures() {
    let source = r#"
func main() -> i32 {
    val t: Tensor<f32, [3]> = [1.0, 2.0, 3.0]
    val k = 2.0f32
    val scaled = t.map(|x| x * k)
    val summed = t.reduce(0.0, |acc, x| acc + x)
    val paired = t.zip(scaled, |a, b| a + b)
    mut kept = 0
    for v in [1, 2, 3, 4].map(|x| x * 10).filter(|x| x > 15) {
        kept += v
    }
    return (scaled.sum() + summed + paired[2]) as i32 + kept
}
"#;
    assert_eq!(run("traversal.nr", source), 12 + 6 + 9 + 90);
}

#[test]
fn a_closure_infers_a_generic_higher_order_call() {
    let source = r#"
func apply<T, U>(value: T, f: (T) -> U) -> U { f(value) }
func first<T, U>(f: (T) -> U, value: T) -> U { f(value) }
func inc(x: i32) -> i32 { x + 1 }
struct Cell { v: i32 }
impl Cell {
    func with<U>(&self, f: (i32) -> U) -> U { f(self.v) }
}
func main() -> i32 {
    val squared = apply(5, |x| x * x)
    val wide: i64 = first(|x| x as i64 * 3, 7)
    val named = apply(4, inc)
    val cell = Cell { v: 6 }
    val read = cell.with(|v| v + 1)
    return squared + wide as i32 + named + read
}
"#;
    assert_eq!(run("generic_hof.nr", source), 25 + 21 + 5 + 7);
}

#[test]
fn a_parameter_nothing_types_is_refused() {
    let err = refused(
        "untyped.nr",
        r#"
func main() -> i32 {
    val f = |x| x + 1
    f(1)
}
"#,
    );
    assert!(err.contains("closure parameter 'x' needs a type"), "{err}");
}

#[test]
fn a_return_in_a_body_with_no_return_type_is_refused() {
    let err = refused(
        "inferred_return.nr",
        r#"
func main() -> i32 {
    val f = |x: i32| {
        if x > 0 { return 1 }
        x
    }
    f(1)
}
"#,
    );
    assert!(err.contains("uses `return` needs a return type"), "{err}");
}

#[test]
fn an_owning_closure_outlives_the_frame_that_built_it() {
    let source = r#"
struct Holder { f: (i32) -> i32 }
enum Step { Run((i32) -> i32), Stop }
func make_adder(n: i32) -> (i32) -> i32 { |x| x + n }
func make_holder(k: i32) -> Holder { Holder { f: |x| x * k } }
func make_length(label: string) -> () -> u64 { move || label.len() }
func reset(h: &mut Holder, k: i32) { h.f = |x| x - k }
func main() -> i32 {
    mut total = 0
    mut i = 0
    while i < 4 {
        val add = make_adder(i)
        val copy = add
        mut h = make_holder(2)
        reset(&mut h, 1)
        val sub = h.f
        val len = make_length("ab" + "cde")
        val step = Step::Run(make_adder(100))
        val stepped = match step {
            Step::Run(f) => f(0),
            Step::Stop => 0
        }
        total += add(1) + copy(1) + sub(10) + len() as i32 + stepped
        i += 1
    }
    return total % 256
}
"#;
    // Each turn: (1 + i) twice, 9, 5, 100.
    let expected: i32 = (0..4).map(|i| 2 * (1 + i) + 9 + 5 + 100).sum();
    assert_eq!(run("escaping.nr", source), expected % 256);
}

#[test]
fn an_escaping_environment_releases_what_it_owns_once() {
    let source = r#"
struct Noisy { id: i32 }
impl Drop for Noisy {
    func drop(&mut self) {
        println("drop {self.id}")
    }
}
func keep(id: i32) -> () -> i32 {
    val n = Noisy { id: id }
    move || n.id
}
func main() -> i32 {
    val first = keep(1)
    val again = first
    val second = keep(2)
    val unused = keep(3)
    first() + again() + second()
}
"#;
    let (stdout, code) = output("escaping_drop.nr", source);
    assert_eq!(code, Some(4));
    for id in [1, 2, 3] {
        assert_eq!(
            stdout.matches(&format!("drop {id}")).count(),
            1,
            "value {id} must be destroyed exactly once, got:\n{stdout}"
        );
    }
}

#[test]
fn a_value_that_may_borrow_cannot_leave_its_scope() {
    for (name, item) in [
        (
            "returned_parameter.nr",
            "func pass(g: (i32) -> i32) -> (i32) -> i32 { g }",
        ),
        (
            "wrapped_parameter.nr",
            "func wrap(g: (i32) -> i32) -> (i32) -> i32 { move |x| g(x) }",
        ),
        (
            "stored_parameter.nr",
            "struct H { f: (i32) -> i32 }\nfunc keep(h: &mut H, g: (i32) -> i32) { h.f = g }",
        ),
        (
            "returned_borrow.nr",
            "func len_of(s: &string) -> () -> u64 { || s.len() }",
        ),
        (
            "returned_block_borrow.nr",
            "func len_of(s: &string) -> () -> u64 {\n    val g = {\n        val r = s\n        move || r.len()\n    }\n    g\n}",
        ),
    ] {
        let err = refused(name, &format!("{item}\nfunc main() -> i32 {{ 0 }}\n"));
        assert!(
            err.contains("lives only as long as that scope"),
            "{name}: {err}"
        );
    }
}
