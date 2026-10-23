// End-to-end tests for function values as `|>` targets and `>>` operands.
//
// Five spellings produce a function value here: a function name, a path to an associated
// function, an instantiated generic `f::<T>`, a closure literal and a bound method
// `receiver.method`. Each is a value on its own (the bound method only inside the two
// operators), a pipeline target, and a composition stage. A composition is a closure, so
// a stage that is a value is captured by the closure capture rules. A tuple variant named
// as its constructor is tested with `.map_err` in `option_result_methods.rs`.

use crate::compile_harness::CompileTest;

const PRELUDE: &str = r#"
struct Point { x: i32 }

impl Point {
    func scale(v: i32) -> i32 { v * 3 }
    func shift(&self, v: i32) -> i32 { v + self.x }
}

func identity<T>(value: T) -> T { value }
func double(v: i32) -> i32 { v * 2 }
func apply(v: i32, f: (i32) -> i32) -> i32 { f(v) }
"#;

fn run(name: &str, main: &str) -> i32 {
    let test = CompileTest::new();
    test.compile_and_run(name, &format!("{PRELUDE}\n{main}"))
        .expect("compile/run failed")
}

fn check_error(name: &str, main: &str) -> String {
    let test = CompileTest::new();
    test.check(name, &format!("{PRELUDE}\n{main}"))
        .expect_err("the program should be refused")
}

#[test]
fn an_instantiated_generic_is_a_value() {
    let exit = run(
        "instance_value.nr",
        r#"
func main() -> i32 {
    val f = identity::<i32>
    val text = identity::<string>
    val s = text("abc")
    f(4) + apply(5, identity::<i32>) + s.len() as i32
}
"#,
    );
    assert_eq!(exit, 4 + 5 + 3);
}

#[test]
fn an_associated_function_is_a_value() {
    let exit = run(
        "assoc_value.nr",
        r#"
func main() -> i32 {
    val f = Point::scale
    f(2) + apply(3, Point::scale)
}
"#,
    );
    assert_eq!(exit, 6 + 9);
}

#[test]
fn an_instance_is_a_pipeline_target() {
    let exit = run(
        "instance_target.nr",
        r#"
func widen<T>(v: i32) -> i64 { v as i64 + 1 }

func main() -> i32 {
    val wide = 4 |> widen::<u8>
    wide as i32
}
"#,
    );
    assert_eq!(exit, 5);
}

#[test]
fn a_closure_target_takes_the_piped_type() {
    let exit = run(
        "closure_target.nr",
        r#"
func main() -> i32 {
    val a = 4 |> (|x| x * 2)
    val b = a
        |> (|x| {
            val y = x + 1
            y * 2
        })
    (|x, y| x + y)(a, b)
}
"#,
    );
    assert_eq!(exit, 8 + 18);
}

#[test]
fn every_function_value_composes() {
    let exit = run(
        "compose_all.nr",
        r#"
func main() -> i32 {
    val p = Point { x: 1 }
    val held = double
    val chain = Point::scale >> identity::<i32> >> (|v| v + 1) >> p.shift >> held >> identity
    chain(2)
}
"#,
    );
    // scale 6, identity 6, +1 7, shift 8, double 16, identity 16.
    assert_eq!(exit, 16);
}

#[test]
fn a_closure_stage_captures_by_the_closure_rules() {
    // A read-only `Copy` capture is copied, so the composition owns everything it holds
    // and may be returned; a written capture is borrowed and sees each call's write.
    let exit = run(
        "compose_capture.nr",
        r#"
func offset_by(n: i32) -> (i32) -> i32 {
    double >> (|v| v + n)
}

func main() -> i32 {
    mut calls = 0
    val counted = double >> (|v| {
        calls += 1
        v + calls
    })
    val escaped = offset_by(5)
    counted(1) + counted(1) + escaped(1)
}
"#,
    );
    assert_eq!(exit, 3 + 4 + 7);
}

#[test]
fn closure_stages_differentiate_where_they_are_written() {
    let test = CompileTest::new();
    let source = r#"
func square(x: f32) -> f32 { x * x }

@grad
func loss(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    val a = w[0] |> (|x| x * 3.0)
    val both = square >> (|x| x + w[1])
    return Tensor::scalar(a + both(w[0]))
}

func main() -> i32 {
    mut w: Tensor<f32, [2]> = [1.0, 2.0]
    val l = loss(&mut w)
    l.backward()
    val g = w.grad()
    // l = 3 + (1 + 2) = 6; dl/dw0 = 3 + 2 w0 = 5; dl/dw1 = 1.
    (l.sum() + g[0] * 10.0 + g[1]) as i32
}
"#;
    let exit = test
        .compile_and_run("grad_stages.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 6 + 50 + 1);
}

#[test]
fn a_move_closure_stage_is_refused() {
    let error = check_error(
        "compose_move.nr",
        r#"
func main() -> i32 {
    val n = 2
    val g = double >> (move |v| v + n)
    g(1)
}
"#,
    );
    assert!(error.contains("compose the binding"), "{error}");
}

#[test]
fn a_composition_of_named_functions_escapes() {
    let exit = run(
        "compose_escape.nr",
        r#"
func prepare() -> (i32) -> i32 { double >> Point::scale >> identity::<i32> }

func main() -> i32 { prepare()(2) }
"#,
    );
    assert_eq!(exit, 12);
}

#[test]
fn a_composition_with_a_context_types_a_generic_first_stage() {
    let exit = run(
        "compose_context.nr",
        r#"
func main() -> i32 {
    val g: (i32) -> i32 = identity >> double
    g(7)
}
"#,
    );
    assert_eq!(exit, 14);
}

#[test]
fn a_bound_method_stage_borrows_its_receiver() {
    // The composition holds the borrow for as long as it lives, so it cannot leave the
    // scope that owns the receiver.
    let error = check_error(
        "compose_borrow.nr",
        r#"
func shifter() -> (i32) -> i32 {
    val p = Point { x: 1 }
    p.shift >> double
}

func main() -> i32 { shifter()(1) }
"#,
    );
    assert!(error.contains("may borrow from the scope"), "{error}");
}

#[test]
fn a_bound_method_on_a_temporary_is_refused() {
    let error = check_error(
        "compose_temporary.nr",
        r#"
func make() -> Point { Point { x: 1 } }

func main() -> i32 {
    val g = make().shift >> double
    g(1)
}
"#,
    );
    assert!(error.contains("borrows its receiver"), "{error}");
}

#[test]
fn a_bare_generic_name_is_not_a_value() {
    let error = check_error(
        "bare_generic.nr",
        r#"
func main() -> i32 {
    val f = identity
    f(1)
}
"#,
    );
    assert!(error.contains("`identity::<T>`"), "{error}");
}

#[test]
fn an_unannotated_first_closure_stage_is_refused() {
    let error = check_error(
        "compose_untyped_closure.nr",
        r#"
func main() -> i32 {
    val g = (|v| v + 1) >> double
    g(1)
}
"#,
    );
    assert!(
        error.contains("closure parameter 'v' needs a type"),
        "{error}"
    );
}

#[test]
fn an_instantiated_method_is_not_a_value_on_its_own() {
    let error = check_error(
        "instance_method_value.nr",
        r#"
func main() -> i32 {
    val p = Point { x: 1 }
    val f = p.shift::<i32>
    0
}
"#,
    );
    assert!(
        error.contains("only a generic function's instance"),
        "{error}"
    );
}
