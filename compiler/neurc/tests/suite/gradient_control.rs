//! `.detach()` and `@no_grad` end to end: a detached value and a `@no_grad` call's result
//! are constants to the derivative, at both orders, and a detach outside any derivative is
//! a move that forgets the gradient.
//!
//! The gradients here are analytic, so each program pins its result in its exit code;
//! `grad_differential` checks the first-order fences against finite differences of the
//! function with the fenced values frozen.
use crate::compile_harness::CompileTest;
use std::process::Command;

fn run(test: &CompileTest, name: &str, source: &str) -> i32 {
    test.compile_and_run(name, source)
        .expect("compile/run failed")
}

fn refused(test: &CompileTest, name: &str, source: &str) -> String {
    let path = test.write_source(name, source);
    test.compile(&path)
        .expect_err("the program compiled, but it must be refused")
}

#[test]
fn a_detached_value_and_a_no_grad_result_are_constants() {
    let test = CompileTest::new();
    let exit = run(
        &test,
        "fenced.nr",
        r#"
@no_grad
func halved(x: &Tensor<f32, [3]>) -> Tensor<f32, [3]> {
    x * 0.5
}

@no_grad
func weight(x: Tensor<f32, [3]>) -> f32 {
    x.max()
}

@grad
func loss(w: &mut Tensor<f32, [3]>) -> Tensor<f32, []> {
    val doubled = w * 2.0
    val z = doubled.detach()
    val h = halved(w)
    val owned = w * 1.0
    val k = weight(owned)
    val total = &z * w + &h * w
    Tensor::scalar(total.sum() * k)
}

func main() -> i32 {
    mut w: Tensor<f32, [3]> = [1.0, 2.0, 3.0]
    val l = loss(&mut w)
    l.backward()
    // (z + h) k with z = 2w, h = w / 2 and k = max(w) = 3: 7.5 w. Through the fences it
    // would be 4.5 w k + |w|^2 at the maximum, nothing like it.
    val g = w.grad()
    return (g[0] * 2.0 + g[2] * 2.0) as i32
}
"#,
    );
    assert_eq!(exit, 15 + 45);
}

#[test]
fn the_fences_hold_for_the_second_derivative() {
    let test = CompileTest::new();
    let exit = run(
        &test,
        "fenced_hessian.nr",
        r#"
@no_grad
func peak(x: &Tensor<f32, [2]>) -> f32 {
    x.max()
}

// z w^2 with z = w^2 frozen: the gradient is 2 z w and the Hessian 2 z, both on the
// diagonal. A fence that only held at first order would give 6 w^2.
@grad(order: 2)
func detached(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    val squared = w * w
    val z = squared.detach()
    val cubic = &z * w * w
    Tensor::scalar(cubic.sum())
}

// m |w|^2 with m = max(w) a constant: gradient 2 m w, Hessian 2 m I.
@grad(order: 2)
func scaled(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    val m = peak(w)
    val squares = w * w
    Tensor::scalar(squares.sum() * m)
}

func main() -> i32 {
    mut w: Tensor<f32, [2]> = [1.0, 2.0]
    val a = detached(&mut w)
    a.backward()
    val g = w.grad()
    val h: &Tensor<f32, [2, 2]> = w.hessian()
    assert(g[0] == 2.0f32 && g[1] == 16.0f32)
    assert(h[0, 0] == 2.0f32 && h[1, 1] == 8.0f32 && h[0, 1] == 0.0f32)

    mut v: Tensor<f32, [2]> = [1.0, 3.0]
    val b = scaled(&mut v)
    b.backward()
    val k = v.hessian()
    assert(v.grad()[1] == 18.0f32)
    return (k[0, 0] + k[1, 1] + k[0, 1]) as i32
}
"#,
    );
    assert_eq!(exit, 12);
}

#[test]
fn a_detach_moves_the_tensor_and_empties_its_gradient_slot() {
    let test = CompileTest::new();
    let path = test.write_source(
        "detach_slot.nr",
        r#"
@grad
func loss(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    val squares = w * w
    Tensor::scalar(squares.sum())
}

func main() -> i32 {
    mut w: Tensor<f32, [2]> = [1.0, 2.0]
    val l = loss(&mut w)
    l.backward()
    assert(w.grad()[1] == 4.0f32)
    val d = w.detach()
    assert(d[1] == 2.0f32)
    val g = d.grad()
    return g[0] as i32
}
"#,
    );
    let exe = test.compile(&path).expect("compile failed");
    let output = Command::new(exe).output().expect("run executable");
    assert_ne!(
        output.status.code(),
        Some(0),
        "the detached slot was not empty"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("empty gradient slot"), "{stderr}");
}

#[test]
fn a_detached_tensor_is_moved_and_a_borrow_cannot_be_detached() {
    let test = CompileTest::new();
    let moved = refused(
        &test,
        "detach_moved.nr",
        r#"
func main() -> i32 {
    val t: Tensor<f32, [2]> = [1.0, 2.0]
    val d = t.detach()
    return (t[0] + d[0]) as i32
}
"#,
    );
    assert!(moved.contains("moved"), "{moved}");
    let borrowed = refused(
        &test,
        "detach_borrowed.nr",
        r#"
func fence(t: &Tensor<f32, [2]>) -> Tensor<f32, [2]> {
    t.detach()
}

func main() -> i32 {
    val t: Tensor<f32, [2]> = [1.0, 2.0]
    return fence(&t)[0] as i32
}
"#,
    );
    assert!(borrowed.contains("detach"), "{borrowed}");
}

#[test]
fn no_grad_is_bare_on_a_free_function_that_is_not_grad() {
    let test = CompileTest::new();
    for (name, source, expected) in [
        (
            "no_grad_with_grad.nr",
            "@grad\n@no_grad\nfunc f(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {\n    Tensor::scalar(w.sum())\n}\nfunc main() -> i32 { 0 }\n",
            "cannot share a function with `@grad`",
        ),
        (
            "no_grad_argument.nr",
            "@no_grad(fast)\nfunc f(x: f32) -> f32 { x }\nfunc main() -> i32 { 0 }\n",
            "takes no arguments",
        ),
        (
            "no_grad_method.nr",
            "struct S { x: f32 }\nimpl S {\n    @no_grad\n    func get(&self) -> f32 { self.x }\n}\nfunc main() -> i32 { 0 }\n",
            "on a method is not supported yet",
        ),
    ] {
        let error = refused(&test, name, source);
        assert!(error.contains(expected), "{name}: {error}");
    }
}

#[test]
fn a_no_grad_call_may_not_write_through_its_argument() {
    let test = CompileTest::new();
    let error = refused(
        &test,
        "no_grad_mut.nr",
        r#"
@no_grad
func bump(x: &mut Tensor<f32, [2]>) -> f32 {
    x.sum()
}

@grad
func loss(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    val k = bump(w)
    Tensor::scalar(w.sum() * k)
}

func main() -> i32 {
    mut w: Tensor<f32, [2]> = [1.0, 2.0]
    val l = loss(&mut w)
    l.backward()
    return 0
}
"#,
    );
    assert!(error.contains("borrows mutably"), "{error}");
}
