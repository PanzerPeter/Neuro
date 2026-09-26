//! `@grad(order: 2)` end to end: `.backward()` fills each differentiated parameter's
//! `.hessian()` beside its `.grad()`, and every way a slot is emptied empties it too.
//!
//! The Hessians here are analytic, so each program pins its result in its exit code;
//! `grad_differential` checks the general case against finite differences of the compiled
//! gradient.
use crate::compile_harness::CompileTest;
use std::process::Command;

const LOSSES: &str = r#"
// The specification's example: (sum x)^2, whose Hessian is 2 everywhere.
@grad(order: 2)
func squared_total(x: &mut Tensor<f32, [3]>) -> Tensor<f32, []> {
    x.sum(axis: 0) * x.sum(axis: 0)
}

// a^3 + a b^2: the Hessian is [[6a, 2b], [2b, 2a]].
@grad(order: 2)
func cubic(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    val a = w[0]
    val b = w[1]
    Tensor::scalar(a * a * a + a * b * b)
}

@grad
func first(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    val s = w * w
    Tensor::scalar(s.sum())
}
"#;

fn run(test: &CompileTest, name: &str, main: &str) -> i32 {
    test.compile_and_run(name, &format!("{LOSSES}\n{main}"))
        .expect("compile/run failed")
}

/// The stderr of a program that must abort, or a panic of the test when it ran to the end.
fn panic_message(test: &CompileTest, name: &str, main: &str) -> String {
    let path = test.write_source(name, &format!("{LOSSES}\n{main}"));
    let exe = test.compile(&path).expect("compile failed");
    let output = Command::new(exe).output().expect("run executable");
    assert_ne!(output.status.code(), Some(0), "the program ran to the end");
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn backward_fills_the_gradient_and_the_hessian() {
    let test = CompileTest::new();
    let exit = run(
        &test,
        "hessian_spec.nr",
        r#"
func main() -> i32 {
    mut x: Tensor<f32, [3]> = [1.0, 2.0, 3.0]
    val loss = squared_total(&mut x)
    loss.backward()
    val g = x.grad()
    val h: &Tensor<f32, [3, 3]> = x.hessian()
    // The gradient is 2 * 6 = 12 per element; the nine Hessian entries are 2 each.
    return g[0] as i32 + h.sum() as i32
}
"#,
    );
    assert_eq!(exit, 12 + 18);
}

#[test]
fn the_hessian_depends_on_the_point() {
    let test = CompileTest::new();
    let exit = run(
        &test,
        "hessian_cubic.nr",
        r#"
func main() -> i32 {
    mut w: Tensor<f32, [2]> = [2.0, 3.0]
    val loss = cubic(&mut w)
    loss.backward()
    val h = w.hessian()
    // [[12, 6], [6, 4]], read as the digits of one number.
    return (h[0, 0] * 1000.0 + h[0, 1] * 100.0 + h[1, 0] * 10.0 + h[1, 1]) as i32 % 256
}
"#,
    );
    assert_eq!(exit, 12_664 % 256);
}

#[test]
fn a_hessian_before_any_backward_panics() {
    let test = CompileTest::new();
    let message = panic_message(
        &test,
        "hessian_empty.nr",
        r#"
func main() -> i32 {
    val w: Tensor<f32, [2]> = [1.0, 2.0]
    val h = w.hessian()
    return h[0, 0] as i32
}
"#,
    );
    assert!(message.contains("empty Hessian slot"), "{message}");
}

#[test]
fn zero_grad_empties_the_hessian_too() {
    let test = CompileTest::new();
    let message = panic_message(
        &test,
        "hessian_zeroed.nr",
        r#"
func main() -> i32 {
    mut w: Tensor<f32, [2]> = [1.0, 2.0]
    val loss = cubic(&mut w)
    loss.backward()
    w.zero_grad()
    val h = w.hessian()
    return h[0, 0] as i32
}
"#,
    );
    assert!(message.contains("empty Hessian slot"), "{message}");
}

/// A first-order `.backward()` replaces the gradient, and a Hessian taken at the earlier
/// point would no longer belong to it, so the slot is emptied rather than left stale.
#[test]
fn a_first_order_backward_empties_the_hessian() {
    let test = CompileTest::new();
    let message = panic_message(
        &test,
        "hessian_stale.nr",
        r#"
func main() -> i32 {
    mut w: Tensor<f32, [2]> = [1.0, 2.0]
    val second = cubic(&mut w)
    second.backward()
    val once = first(&mut w)
    once.backward()
    val h = w.hessian()
    return h[0, 0] as i32
}
"#,
    );
    assert!(message.contains("empty Hessian slot"), "{message}");
}

/// Newton's method on a separable quartic: each step solves `H d = g` on the diagonal
/// Hessian, and the minimum at 1 is reached in a handful of steps inside a `pool`.
#[test]
fn newton_steps_converge_inside_a_pool() {
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "hessian_newton.nr",
            r#"
@grad(order: 2)
func quartic(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    val d = w - 1.0
    val squared = &d * &d
    val fourth = &squared * &squared
    Tensor::scalar(fourth.sum() + squared.sum())
}

func main() -> i32 {
    mut w: Tensor<f32, [2]> = [3.0, -2.0]
    mut step = 0
    while step < 20 {
        pool {
            val loss = quartic(&mut w)
            loss.backward()
            // Read through temporaries, so no borrow of `w` outlives the update.
            val d: Tensor<f32, [2]> = [
                w.grad()[0] / w.hessian()[0, 0],
                w.grad()[1] / w.hessian()[1, 1]
            ]
            w -= d
        }
        step += 1
    }
    val miss = (w[0] - 1.0).abs() + (w[1] - 1.0).abs()
    return if miss < 0.0001f32 { 1 } else { 0 }
}
"#,
        )
        .expect("compile/run failed");
    assert_eq!(exit, 1);
}

#[test]
fn an_order_with_no_accessor_is_refused() {
    let test = CompileTest::new();
    let error = test
        .check(
            "hessian_order3.nr",
            r#"
@grad(order: 3)
func loss(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    Tensor::scalar(w.sum())
}

func main() -> i32 { return 0 }
"#,
        )
        .expect_err("order 3 has no accessor");
    assert!(error.contains("derivative order"), "{error}");
}
