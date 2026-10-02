// Tensor bodies routed through mlir-backend.
//
// A routed body must give the same answers the LLVM backend's own would, which the run
// tests check; the IR assertions prove which backend computed each body.

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

/// Integer element arithmetic, which MLIR computes with the LLVM backend's checks.
const INTEGER: &str = r#"
func add(a: Tensor<i32, [2]>, b: Tensor<i32, [2]>) -> Tensor<i32, [2]> {
    a + b
}

func main() -> i32 {
    val s = add([1, 2], [3, 4])
    return s[1]
}
"#;

/// A linked integer body that fails a check: an overflow in `add`, a zero divisor in `div`.
const FAILING: &str = r#"
func add(a: Tensor<i32, [2]>, b: Tensor<i32, [2]>) -> Tensor<i32, [2]> {
    a + b
}

func div(a: &Tensor<i64, [2]>, b: &Tensor<i64, [2]>) -> Tensor<i64, [2]> {
    a / b
}

func main() -> i32 {
    val s = add([1, 2147483647], [3, 4])
    println("{s[1]}")
    val n: Tensor<i64, [2]> = [-9223372036854775807 - 1, 5]
    val d: Tensor<i64, [2]> = [-1, 0]
    val q = div(&n, &d)
    println("{q[0]}")
    return 0
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

#[test]
fn integer_bodies_are_computed_by_mlir() {
    let ir = emit_llvm_ir(&CompileTest::new(), "integer.nr", INTEGER);
    assert!(
        ir.contains("define internal void @__neuro_mlir_add(")
            && ir.contains("@llvm.sadd.with.overflow.i32"),
        "an integer body keeps its overflow check through MLIR:\n{ir}"
    );
}

/// A failed check in a linked body is the LLVM backend's own panic, at the operator: an
/// overflow on the debug tier only, and a zero divisor on every tier.
#[test]
fn a_linked_body_fails_its_checks_as_the_llvm_backend_does() {
    let test = CompileTest::new();
    let source = test.write_source("failing.nr", FAILING);
    for (level, stdout, panic, at) in [
        ("0", "", "panic: integer overflow at ", "/failing.nr:3:5\n"),
        (
            "2",
            "-2147483645\n",
            "panic: division by zero at ",
            "/failing.nr:7:5\n",
        ),
    ] {
        let exe = source.with_extension(format!("o{level}"));
        let built = Command::new(env!("CARGO_BIN_EXE_neurc"))
            .args(["compile", "-O", level, "-o"])
            .arg(&exe)
            .arg(&source)
            .output()
            .expect("neurc runs");
        assert!(built.status.success(), "{built:?}");
        let output = Command::new(&exe).output().expect("the program runs");
        assert_eq!(String::from_utf8_lossy(&output.stdout), stdout, "-O{level}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.starts_with(panic) && stderr.ends_with(at) && stderr.lines().count() == 1,
            "-O{level}: {stderr}"
        );
    }
}
