// Tensor bodies routed through mlir-backend when neurc is built with `--features mlir`.
//
// The programs run on both builds and must give the same answers, which is the point:
// a routed body is interchangeable with the LLVM backend's own. Only the IR assertions
// differ by feature, and they are what prove which backend computed each body.

use crate::compile_harness::CompileTest;
use std::path::Path;
use std::process::Command;

/// Straight-line float tensor arithmetic over owned, borrowed and scalar parameters.
/// Each check returns a distinct exit code, so a wrong element names itself.
const ROUTABLE: &str = r#"
func blend(a: &Tensor<f32, [2, 3]>, b: &Tensor<f32, [2, 3]>, t: f32) -> Tensor<f32, [2, 3]> {
    val scaled = a * t
    return scaled + b
}

func project(w: Tensor<f64, [2, 3]>, x: Tensor<f64, [3, 2]>) -> Tensor<f64, [2, 2]> {
    w @ x
}

func shift(row: &Tensor<f32, [3]>, m: &Tensor<f32, [2, 3]>) -> Tensor<f32, [2, 3]> {
    m - row
}

func main() -> i32 {
    val a: Tensor<f32, [2, 3]> = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]
    val b: Tensor<f32, [2, 3]> = [[0.5, 0.5, 0.5], [1.0, 1.0, 1.0]]
    val c = blend(&a, &b, 2.0f32)
    if c[0, 0] != 2.5f32 { return 1 }
    if c[1, 2] != 13.0f32 { return 2 }
    // Borrowed, so still ours after the call.
    if a[1, 1] != 5.0f32 { return 3 }

    val w: Tensor<f64, [2, 3]> = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]
    val x: Tensor<f64, [3, 2]> = [[7.0, 8.0], [9.0, 10.0], [11.0, 12.0]]
    val p = project(w, x)
    if p[0, 0] != 58.0 { return 4 }
    if p[1, 1] != 154.0 { return 5 }

    val row: Tensor<f32, [3]> = [1.0, 2.0, 3.0]
    val s = shift(&row, &a)
    if s[1, 0] != 3.0f32 { return 6 }
    if s[0, 2] != 0.0f32 { return 7 }
    return 0
}
"#;

/// Integer element arithmetic, which the LLVM backend guards and MLIR's `arith` does not.
const INTEGER: &str = r#"
func add(a: Tensor<i32, [2]>, b: Tensor<i32, [2]>) -> Tensor<i32, [2]> {
    a + b
}

func main() -> i32 {
    val s = add([1, 2], [3, 4])
    return s[1]
}
"#;

fn emit_llvm_ir(test: &CompileTest, filename: &str, source: &str) -> String {
    let source_path = test.write_source(filename, source);
    let ir_path = source_path.with_extension("ll");
    let output = Command::new(env!("CARGO_BIN_EXE_neurc"))
        .arg("compile")
        .arg("--emit")
        .arg("llvm-ir")
        .arg("-O0")
        .arg("-o")
        .arg(&ir_path)
        .arg(&source_path)
        .output()
        .expect("Failed to execute neurc compile");
    assert!(
        output.status.success(),
        "Expected --emit llvm-ir to succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::read_to_string(Path::new(&ir_path)).expect("--emit llvm-ir must write the output")
}

#[test]
fn routable_bodies_compute_the_same_answers() {
    let result = CompileTest::new().compile_and_run("routable.nr", ROUTABLE);
    assert_eq!(result, Ok(0));
}

#[test]
fn integer_bodies_compute_the_same_answers() {
    let result = CompileTest::new().compile_and_run("integer.nr", INTEGER);
    assert_eq!(result, Ok(6));
}

#[cfg(feature = "mlir")]
#[test]
fn float_bodies_are_computed_by_mlir() {
    let ir = emit_llvm_ir(&CompileTest::new(), "routable.nr", ROUTABLE);
    for symbol in ["blend", "project", "shift"] {
        assert!(
            ir.contains(&format!("define internal void @__neuro_mlir_{symbol}(")),
            "expected `{symbol}`'s body from MLIR:\n{ir}"
        );
        assert!(
            ir.contains(&format!("define ptr @{symbol}(")),
            "expected `{symbol}` to keep its Neuro ABI:\n{ir}"
        );
    }
}

#[cfg(feature = "mlir")]
#[test]
fn integer_bodies_keep_their_guards_on_the_llvm_backend() {
    let ir = emit_llvm_ir(&CompileTest::new(), "integer.nr", INTEGER);
    assert!(
        !ir.contains("__neuro_mlir_"),
        "an integer body must not lose its overflow guard to MLIR:\n{ir}"
    );
}

#[cfg(not(feature = "mlir"))]
#[test]
fn without_mlir_every_body_is_the_llvm_backends() {
    let ir = emit_llvm_ir(&CompileTest::new(), "routable.nr", ROUTABLE);
    assert!(
        !ir.contains("__neuro_mlir_"),
        "a build without the mlir feature has no MLIR path:\n{ir}"
    );
}
