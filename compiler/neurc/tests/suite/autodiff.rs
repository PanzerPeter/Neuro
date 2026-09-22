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
