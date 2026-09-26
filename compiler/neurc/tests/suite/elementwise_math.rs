// Elementwise math: `.exp()`, `.log()`, `.sqrt()`, `.tanh()`, `.abs()` and `.pow(p)` on
// float scalars and float tensors, and their derivatives in a `@grad` body.
//
// Every program answers through its exit code, one digit or bit per check, so a wrong
// value is identifiable from the code alone. Values are compared within a tolerance
// wide enough for `f32` rounding and far too narrow for a wrong function.
use crate::compile_harness::CompileTest;

/// `|a - b| < 1e-4`, written in Neuro so every program can share it.
const CLOSE: &str = r#"
func close(a: f64, b: f64) -> bool {
    val d = a - b
    return d < 0.0001 && d > -0.0001
}
"#;

fn run(test: &CompileTest, name: &str, body: &str) -> i32 {
    let source = format!("{CLOSE}{body}");
    test.compile_and_run(name, &source)
        .unwrap_or_else(|e| panic!("{name} failed: {e}"))
}

#[test]
fn scalar_math_matches_the_c_library() {
    let test = CompileTest::new();
    let exit = run(
        &test,
        "math_scalars.nr",
        r#"
func main() -> i32 {
    val x: f64 = 2.0
    val h: f32 = 0.25
    mut code: i32 = 0
    if close(x.exp(), 7.38905609893065) { code += 1 }
    if close(x.log(), 0.6931471805599453) { code += 2 }
    if close(x.sqrt(), 1.4142135623730951) { code += 4 }
    if close(x.tanh(), 0.9640275800758169) { code += 8 }
    if close((0.0 - x).abs(), 2.0) { code += 16 }
    if close(x.pow(10.0), 1024.0) { code += 32 }
    if close((h.sqrt() + h.pow(0.5)) as f64, 1.0) { code += 64 }
    return code
}
"#,
    );
    assert_eq!(exit, 127);
}

#[test]
fn tensor_math_applies_per_element_and_keeps_the_receiver() {
    let test = CompileTest::new();
    // `t` is read by every call and is still whole at the end; the chained `.exp().log()`
    // releases its temporary and gives back the input.
    let exit = run(
        &test,
        "math_tensors.nr",
        r#"
func main() -> i32 {
    val t: Tensor<f64, [2, 2]> = [[1.0, -4.0], [0.0, 9.0]]
    val roots = t.abs().sqrt()
    val round_trip = (&t * 1.0).exp().log()
    val squares = t.pow(2.0)
    mut code: i32 = 0
    if close(roots[0, 1], 2.0) && close(roots[1, 1], 3.0) && close(roots[1, 0], 0.0) { code += 1 }
    if close(round_trip.sum(), t.sum()) { code += 2 }
    if close(squares.sum(), 98.0) { code += 4 }
    if close(t.tanh()[0, 0], 0.7615941559557649) { code += 8 }
    return code
}
"#,
    );
    assert_eq!(exit, 15);
}

#[test]
fn half_precision_tensors_are_computed_widened() {
    let test = CompileTest::new();
    let exit = run(
        &test,
        "math_half.nr",
        r#"
func main() -> i32 {
    val b: Tensor<bf16, [2]> = [4.0bf16, 16.0bf16]
    val h: Tensor<f16, [2]> = [0.5f16, 1.5f16]
    val roots = b.sqrt()
    val powered = b.pow(0.5bf16)
    val curved = h.tanh()
    mut code: i32 = 0
    if roots[0] as f64 == 2.0 && roots[1] as f64 == 4.0 { code += 1 }
    if powered[1] as f64 == 4.0 { code += 2 }
    // f16 carries about three decimal digits.
    val d = curved[1] as f64 - 0.9051482536448664
    if d < 0.001 && d > -0.001 { code += 4 }
    return code
}
"#,
    );
    assert_eq!(exit, 7);
}

#[test]
fn out_of_domain_inputs_follow_ieee_754() {
    let test = CompileTest::new();
    let exit = run(
        &test,
        "math_domain.nr",
        r#"
func main() -> i32 {
    val zero: f64 = 0.0
    val minus: f64 = 0.0 - 1.0
    mut code: i32 = 0
    if zero.log() < 0.0 - 1.0e300 { code += 1 }
    if minus.log().is_nan() { code += 2 }
    if minus.sqrt().is_nan() { code += 4 }
    return code
}
"#,
    );
    assert_eq!(exit, 7);
}

#[test]
fn every_rule_differentiates_through_backward() {
    let test = CompileTest::new();
    // The hand derivative of each term at w = [0.5, 1.0, 2.0]: exp, 1/x, 1/(2 sqrt x),
    // 1 - tanh^2 and 3 x^2 per element, plus exp + 2x + sign for the scalar terms of w0.
    let exit = run(
        &test,
        "math_grad.nr",
        r#"
@grad
func loss(w: &mut Tensor<f32, [3]>) -> Tensor<f32, []> {
    val e = w.exp()
    val l = w.log()
    val s = w.sqrt()
    val t = w.tanh()
    val p = w.pow(3.0)
    val x = w[0]
    val scalar = x.exp() + x.pow(2.0) + x.abs()
    return Tensor::scalar(e.sum() + l.sum() + s.sum() + t.sum() + p.sum() + scalar)
}

func main() -> i32 {
    mut w: Tensor<f32, [3]> = [0.5, 1.0, 2.0]
    val l = loss(&mut w)
    l.backward()
    val g = w.grad()
    mut code: i32 = 0
    if close(g[0] as f64, 9.540997055552731) { code += 1 }
    if close(g[1] as f64, 7.6382561700730705) { code += 2 }
    if close(g[2] as f64, 20.31326031437709) { code += 4 }
    return code
}
"#,
    );
    assert_eq!(exit, 7);
}

#[test]
fn the_gradient_of_abs_is_zero_at_exactly_zero() {
    let test = CompileTest::new();
    let exit = run(
        &test,
        "math_abs_grad.nr",
        r#"
@grad
func l1(w: &mut Tensor<f32, [3]>) -> Tensor<f32, []> {
    return Tensor::scalar(w.abs().sum())
}

func main() -> i32 {
    mut w: Tensor<f32, [3]> = [-2.0, 0.0, 3.0]
    val l = l1(&mut w)
    l.backward()
    val g = w.grad()
    mut code: i32 = 0
    if g[0] == -1.0 { code += 1 }
    if g[1] == 0.0 { code += 2 }
    if g[2] == 1.0 { code += 4 }
    return code
}
"#,
    );
    assert_eq!(exit, 7);
}

#[test]
fn math_on_half_precision_in_a_grad_body_is_refused_where_it_is() {
    let test = CompileTest::new();
    let source = r#"
@grad(wrt: [w])
func loss(w: &mut Tensor<f32, [2]>, h: &Tensor<bf16, [2]>) -> Tensor<f32, []> {
    val r = h.sqrt()
    return Tensor::scalar(w.sum())
}
"#;
    let diagnostics = test
        .check("math_grad_half.nr", source)
        .expect_err("half-precision elements have no derivative");
    assert!(
        diagnostics.contains("elementwise math on half-precision elements"),
        "{diagnostics}"
    );
    assert!(diagnostics.contains(":4:13"), "{diagnostics}");
}

#[test]
fn an_integer_receiver_has_no_math() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val n: i32 = 9
    return n.sqrt()
}
"#;
    let diagnostics = test
        .check("math_integer.nr", source)
        .expect_err("an integer has no `.sqrt()`");
    assert!(diagnostics.contains("sqrt"), "{diagnostics}");
}
