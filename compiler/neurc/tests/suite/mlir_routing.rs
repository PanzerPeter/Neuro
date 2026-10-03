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

/// The outlined operations of an IR module: each defined from MLIR, and none from the LLVM
/// backend's own loops.
fn outlined_from_mlir(ir: &str) -> usize {
    ir.lines()
        .filter(|line| line.starts_with("define internal void @__neuro_mlir___tensor_op_"))
        .count()
}

#[test]
fn float_bodies_are_computed_by_mlir() {
    let ir = emit_llvm_ir(&CompileTest::new(), "routable.nr", ROUTABLE);
    // `a * t`, `scaled + b`, `w @ x` and `m - row`.
    assert_eq!(outlined_from_mlir(&ir), 4, "{ir}");
    for symbol in ["blend", "project", "shift"] {
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
        outlined_from_mlir(&ir) == 1 && ir.contains("@llvm.sadd.with.overflow.i32"),
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
        // Windows writes text-mode `\r\n` and names the source with `\` separators.
        let stdout_text = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
        assert_eq!(stdout_text, stdout, "-O{level}");
        let stderr = String::from_utf8_lossy(&output.stderr)
            .replace("\r\n", "\n")
            .replace('\\', "/");
        assert!(
            stderr.starts_with(panic) && stderr.ends_with(at) && stderr.lines().count() == 1,
            "-O{level}: {stderr}"
        );
    }
}

/// Run `source` built at `-O0`, returning its exit code, stdout and stderr with Windows'
/// line endings and path separators normalized.
fn run_built(test: &CompileTest, filename: &str, source: &str) -> (Option<i32>, String, String) {
    let exe = test
        .compile(&test.write_source(filename, source))
        .expect("the program compiles");
    let output = Command::new(exe).output().expect("the program runs");
    let normalize = |bytes: &[u8]| {
        String::from_utf8_lossy(bytes)
            .replace("\r\n", "\n")
            .replace('\\', "/")
    };
    (
        output.status.code(),
        normalize(&output.stdout),
        normalize(&output.stderr),
    )
}

/// `%` between tensors is computed in MLIR like the other operators: `fmod` for floats, a
/// checked remainder for integers whose zero divisor is the host's panic at the operator.
#[test]
fn a_tensor_remainder_keeps_the_hosts_answers_and_its_panic() {
    const SOURCE: &str = r#"
func main() -> i32 {
    val f: Tensor<f32, [3]> = [7.5, -7.5, 1.0e9]
    val g = &f % 2.0f32
    mut i: Tensor<i32, [3]> = [7, -7, 9]
    i %= 4
    println("{g[0]} {g[1]} {g[2]} {i[0]} {i[1]} {i[2]}")
    val zero: Tensor<i32, [3]> = [1, 0, 1]
    val r = &i % &zero
    return r[0]
}
"#;
    let (code, stdout, stderr) = run_built(&CompileTest::new(), "remainder.nr", SOURCE);
    assert_eq!(stdout, "1.5 -1.5 0.0 3 -3 1\n");
    assert_ne!(code, Some(0));
    assert!(
        stderr.starts_with("panic: remainder by zero at ")
            && stderr.ends_with("remainder.nr:9:13\n"),
        "{stderr}"
    );
}

/// A traversal calls its function on the host through the function value, so a function
/// reached through a local runs too, and one with a side effect sees the elements in
/// row-major order.
#[test]
fn a_traversal_calls_its_function_in_row_major_order() {
    const SOURCE: &str = r#"
func main() -> i32 {
    val t: Tensor<i32, [2, 2]> = [[1, 2], [3, 4]]
    val shout = |x: i32| -> i32 {
        println("{x}")
        x * 10
    }
    val loud = t.map(shout)
    val sum = t.reduce(0, |acc: i32, x: i32| -> i32 { acc * 2 + x })
    return loud[1, 0] + sum
}
"#;
    let (code, stdout, _) = run_built(&CompileTest::new(), "row_major.nr", SOURCE);
    assert_eq!(stdout, "1\n2\n3\n4\n");
    assert_eq!(code, Some(30 + 26));
}

/// A fold whose accumulator is a struct, which no MLIR body carries, still walks every
/// element in row-major order.
#[test]
fn a_fold_into_a_struct_walks_every_element() {
    const SOURCE: &str = r#"
struct Tally {
    count: i32,
    last: f32,
}

func main() -> i32 {
    val t: Tensor<f32, [2, 3]> = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]
    val tally = t.reduce(Tally { count: 0, last: 0.0 }, |acc: Tally, x: f32| -> Tally {
        Tally { count: acc.count + 1, last: x }
    })
    return tally.count * 10 + tally.last as i32
}
"#;
    let (code, _, _) = run_built(&CompileTest::new(), "tally.nr", SOURCE);
    assert_eq!(code, Some(66));
}

/// A `bool` tensor is sliced and transposed by moving its elements, as a number tensor is.
#[test]
fn a_bool_tensor_slices_and_transposes() {
    const SOURCE: &str = r#"
func main() -> i32 {
    val m: Tensor<bool, [2, 3]> = [[true, false, false], [false, true, true]]
    val flipped = m.clone().t()
    val row = m[1, 1..3]
    if flipped[2, 1] && row[0] && !flipped[1, 0] { 0 } else { 1 }
}
"#;
    let (code, _, _) = run_built(&CompileTest::new(), "bools.nr", SOURCE);
    assert_eq!(code, Some(0));
}
