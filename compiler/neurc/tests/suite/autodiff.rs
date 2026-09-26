//! `@grad` functions end to end.
//!
//! Whether the generated derivative is numerically right is checked against finite
//! differences by the `grad_differential` target. These pin the driver's side: an
//! annotated function still compiles and runs as its primal, its derivative is emitted as
//! a real symbol, and a body the transform refuses is a located diagnostic.
use crate::compile_harness::CompileTest;
use std::process::Command;

const LOSS: &str = r#"
@grad
func loss(w: &mut Tensor<f32, [2]>, scale: f32) -> Tensor<f32, []> {
    val squares = w * w
    return Tensor::scalar(squares.sum() * scale)
}
"#;

#[test]
fn a_grad_function_still_runs_as_its_primal() {
    let test = CompileTest::new();
    let source = format!(
        "{LOSS}
func main() -> i32 {{
    mut w: Tensor<f32, [2]> = [1.0, 2.0]
    val l = loss(&mut w, 2.0f32)
    l.sum() as i32
}}
"
    );
    let exit = test
        .compile_and_run("grad_primal.nr", &source)
        .expect("compile/run failed");
    assert_eq!(exit, 10);
}

#[test]
fn the_derivative_is_emitted_beside_the_primal() {
    let test = CompileTest::new();
    let source_path = test.write_source("grad_symbols.nr", LOSS);
    let ir_path = source_path.with_extension("ll");
    let output = Command::new(env!("CARGO_BIN_EXE_neurc"))
        .args(["compile", "--emit", "llvm-ir", "-o"])
        .arg(&ir_path)
        .arg(&source_path)
        .output()
        .expect("run neurc");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let ir = std::fs::read_to_string(&ir_path).expect("read IR");
    assert!(
        ir.contains("define") && ir.contains("@__loss__rev("),
        "{ir}"
    );
}

#[test]
fn a_body_the_transform_refuses_is_reported_where_it_is() {
    let test = CompileTest::new();
    let source = r#"
@grad
func loss(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    val top = w.max()
    return Tensor::scalar(top)
}
"#;
    let diagnostics = test
        .check("grad_refused.nr", source)
        .expect_err("a `.max()` in a `@grad` body has no derivative rule yet");
    assert!(
        diagnostics.contains("cannot differentiate"),
        "{diagnostics}"
    );
    assert!(diagnostics.contains(":4:15"), "{diagnostics}");
}

#[test]
fn a_signature_the_transform_cannot_serve_is_a_type_error() {
    let test = CompileTest::new();
    let source = r#"
@grad
func loss(w: &Tensor<f32, [2]>) -> Tensor<f32, []> {
    Tensor::scalar(w.sum())
}
"#;
    let diagnostics = test
        .check("grad_shared_param.nr", source)
        .expect_err("a differentiated parameter must be `&mut`");
    assert!(diagnostics.contains("`&mut`"), "{diagnostics}");
}

#[test]
fn a_grad_function_with_branches_and_loops_still_runs_as_its_primal() {
    let test = CompileTest::new();
    let source = r#"
@grad
func clipped_power(w: &mut Tensor<f32, [2]>, rounds: i32) -> Tensor<f32, []> {
    mut acc = w * 1.0
    mut done = 0
    while done < rounds {
        acc = &acc * w
        done += 1
    }
    val total = acc.sum()
    if total > 100.0 { return Tensor::scalar(100.0f32) }
    return Tensor::scalar(total)
}

func main() -> i32 {
    mut w: Tensor<f32, [2]> = [2.0, 3.0]
    val small = clipped_power(&mut w, 2)
    val clipped = clipped_power(&mut w, 5)
    (small.sum() + clipped.sum()) as i32
}
"#;
    let exit = test
        .compile_and_run("grad_control_flow.nr", source)
        .expect("compile/run failed");
    // 2^3 + 3^3 = 35, and 2^6 + 3^6 = 793 clips to 100.
    assert_eq!(exit, 135);
}

#[test]
fn a_loop_jump_in_a_grad_body_is_reported_where_it_is() {
    let test = CompileTest::new();
    let source = r#"
@grad
func loss(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    mut acc = w * 1.0
    while acc.sum() < 10.0 {
        acc = &acc * w
        continue
    }
    return Tensor::scalar(acc.sum())
}
"#;
    let diagnostics = test
        .check("grad_loop_jump.nr", source)
        .expect_err("a `continue` in a `@grad` body has no derivative rule");
    assert!(
        diagnostics.contains("cannot differentiate"),
        "{diagnostics}"
    );
    assert!(diagnostics.contains(":7:9"), "{diagnostics}");
}

/// A shape-generic `@grad` template is differentiated once per instance: each shape the
/// program calls it at gets its own derivative, and the primal still runs at both.
#[test]
fn a_generic_grad_function_gets_a_derivative_per_instance() {
    let test = CompileTest::new();
    let source = r#"
@grad
func loss<N>(w: &mut Tensor<f32, [N]>, scale: f32) -> Tensor<f32, []> {
    val first = w[0]
    val second = w[1]
    return Tensor::scalar((first * first + second * second) * scale)
}

func main() -> i32 {
    mut a: Tensor<f32, [2]> = [1.0, 2.0]
    mut b: Tensor<f32, [3]> = [1.0, 2.0, 3.0]
    val la = loss(&mut a, 2.0f32)
    val lb = loss(&mut b, 1.0f32)
    (la.sum() + lb.sum()) as i32
}
"#;
    let exit = test
        .compile_and_run("grad_generic.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 10 + 5);

    let source_path = test.write_source("grad_generic_ir.nr", source);
    let ir_path = source_path.with_extension("ll");
    let output = Command::new(env!("CARGO_BIN_EXE_neurc"))
        .args(["compile", "--emit", "llvm-ir", "-o"])
        .arg(&ir_path)
        .arg(&source_path)
        .output()
        .expect("run neurc");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let ir = std::fs::read_to_string(&ir_path).expect("read IR");
    let derivatives = ir
        .lines()
        .filter(|line| line.starts_with("define") && line.contains("__rev("))
        .count();
    assert_eq!(derivatives, 2, "one derivative per instance:\n{ir}");
}

/// A `@grad` body may call user functions, declared before or after it, and the
/// derivative is still emitted beside a primal that runs as written.
#[test]
fn a_grad_function_calling_helpers_runs_and_gets_a_derivative() {
    let test = CompileTest::new();
    let source = r#"
@grad
func loss(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    val h = w * 1.0
    return Tensor::scalar(squared_norm(&h) + larger(w[0], w[1]))
}

func squared_norm(x: &Tensor<f32, [2]>) -> f32 {
    val squares = x * x
    return squares.sum()
}

func larger(a: f32, b: f32) -> f32 {
    if a > b { return a }
    return b
}

func main() -> i32 {
    mut w: Tensor<f32, [2]> = [1.0, 2.0]
    val l = loss(&mut w)
    l.sum() as i32
}
"#;
    let exit = test
        .compile_and_run("grad_calls.nr", source)
        .expect("compile/run failed");
    // 1 + 4, plus the larger element 2.
    assert_eq!(exit, 7);

    let source_path = test.write_source("grad_calls_ir.nr", source);
    let ir_path = source_path.with_extension("ll");
    let output = Command::new(env!("CARGO_BIN_EXE_neurc"))
        .args(["compile", "--emit", "llvm-ir", "-o"])
        .arg(&ir_path)
        .arg(&source_path)
        .output()
        .expect("run neurc");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let ir = std::fs::read_to_string(&ir_path).expect("read IR");
    assert!(ir.contains("@__loss__rev("), "{ir}");
}

#[test]
fn a_recursive_call_in_a_grad_body_is_reported_where_it_recurses() {
    let test = CompileTest::new();
    let source = r#"
func halve(x: f32, n: i32) -> f32 {
    if n > 0 { return halve(x * 0.5, n - 1) }
    return x
}

@grad
func loss(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    return Tensor::scalar(halve(w.sum(), 3))
}
"#;
    let diagnostics = test
        .check("grad_recursion.nr", source)
        .expect_err("recursion cannot be inlined into a derivative");
    assert!(diagnostics.contains("recursive call"), "{diagnostics}");
    assert!(diagnostics.contains(":3:23"), "{diagnostics}");
}

/// A `@grad` body may call a closure, a composition, a helper taking a function, and a
/// traversal, and a function-typed parameter of the `@grad` function gets the target each
/// `.backward()` call passes it.
#[test]
fn a_grad_body_calling_function_values_backpropagates_through_them() {
    let test = CompileTest::new();
    let source = r#"
func square(x: f32) -> f32 { x * x }
func apply(f: (f32) -> f32, x: f32) -> f32 { f(x) }

@grad
func loss(w: &mut Tensor<f32, [2]>, f: (f32) -> f32) -> Tensor<f32, []> {
    val s = w[1] * 2.0
    val scaled = |x: f32| -> f32 { x * s }
    val both = square >> square
    val squares = w.map(|x: f32| -> f32 { x * x })
    val total = squares.reduce(0.0f32, |acc: f32, x: f32| -> f32 { acc + x })
    return Tensor::scalar(scaled(w[0]) + both(w[0]) + apply(f, w[1]) + total)
}

func main() -> i32 {
    mut w: Tensor<f32, [2]> = [1.0, 2.0]
    val k = 3.0f32
    val l = loss(&mut w, |x: f32| -> f32 { x * k })
    l.backward()
    val g = w.grad()
    // l = 4 + 1 + 6 + 5; dl/dw0 = s + 4 w0^3 + 2 w0 = 10, dl/dw1 = 2 w0 + k + 2 w1 = 9.
    (l.sum() + g[0] * 10.0 + g[1]) as i32
}
"#;
    let exit = test
        .compile_and_run("grad_function_values.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 16 + 100 + 9);
}

#[test]
fn a_function_value_chosen_at_run_time_is_reported_where_it_is_chosen() {
    let test = CompileTest::new();
    let source = r#"
func square(x: f32) -> f32 { x * x }

@grad
func loss(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    val f = if w[0] > 1.0 { square >> square } else { |x: f32| -> f32 { x } }
    return Tensor::scalar(f(w[1]))
}
"#;
    let diagnostics = test
        .check("grad_run_time_target.nr", source)
        .expect_err("a derivative cannot follow a target chosen at run time");
    assert!(diagnostics.contains("chosen at run time"), "{diagnostics}");
    assert!(diagnostics.contains(":6:13"), "{diagnostics}");
}

#[test]
fn wrt_fills_the_slots_of_what_it_lists_and_no_others() {
    let test = CompileTest::new();
    let source = r#"
struct Layer { export w: Tensor<f32, [2]> }

struct Net {
    export layer: Layer,
    export heads: [Tensor<f32, [2]>; 2]
}

impl Net {
    @grad(wrt: [self.layer.w, self.heads[1], k])
    func loss(&mut self, k: &mut Tensor<f32, [2]>, frozen: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
        val w = self.layer.w.clone()
        val p = w * k
        return Tensor::scalar(p.sum() + self.heads[1][0] * frozen.sum())
    }
}

func main() -> i32 {
    mut net = Net { layer: Layer { w: [1.0, 2.0] }, heads: [[0.0, 0.0], [3.0, 4.0]] }
    mut k: Tensor<f32, [2]> = [5.0, 6.0]
    mut frozen: Tensor<f32, [2]> = [1.0, 1.0]
    val l = net.loss(&mut k, &mut frozen)
    l.backward()
    val w = net.layer.w.grad()
    val h = net.heads[1].grad()
    val g = k.grad()
    // l = 17 + 6; dl/dw = k, dl/dheads[1] = [2, 0], dl/dk = w.
    assert(w[0] == 5.0f32 && w[1] == 6.0f32)
    assert(h[0] == 2.0f32 && h[1] == 0.0f32)
    assert(g[0] == 1.0f32 && g[1] == 2.0f32)
    l.sum() as i32
}
"#;
    let exit = test
        .compile_and_run("grad_wrt.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 23);
}

#[test]
fn a_tensor_wrt_leaves_out_has_an_empty_slot() {
    let test = CompileTest::new();
    let source = r#"
@grad(wrt: [b])
func loss(w: &mut Tensor<f32, [2]>, b: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    val p = w * b
    return Tensor::scalar(p.sum())
}

func main() -> i32 {
    mut w: Tensor<f32, [2]> = [1.0, 2.0]
    mut b: Tensor<f32, [2]> = [3.0, 4.0]
    val l = loss(&mut w, &mut b)
    l.backward()
    val g = w.grad()
    0
}
"#;
    let exe = test.write_source("grad_wrt_empty.nr", source);
    let exe = test.compile(&exe).expect("compile failed");
    let output = Command::new(&exe).output().expect("run");
    assert!(
        !output.status.success(),
        "reading an unlisted gradient must panic"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("empty gradient slot"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn a_wrt_path_through_a_private_field_is_reported_at_the_entry() {
    let test = CompileTest::new();
    let source = r#"
struct Net { w: Tensor<f32, [2]> }

impl Net {
    @grad(wrt: [self.w])
    func loss(&mut self) -> Tensor<f32, []> {
        return Tensor::scalar(self.w.sum())
    }
}
"#;
    let diagnostics = test
        .check("grad_wrt_private.nr", source)
        .expect_err("a private field cannot be named in `wrt:`");
    assert!(diagnostics.contains("private field"), "{diagnostics}");
    assert!(diagnostics.contains(":5:17"), "{diagnostics}");
}
