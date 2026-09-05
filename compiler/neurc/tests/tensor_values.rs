// Tensor value construction (Phase 2B): nested-array-literal coercion and the
// six construction helpers, end to end through `neurc compile` and the linked binary.
mod common;

use common::CompileTest;

/// Compile and run `source`, returning its exit code.
fn run_program(name: &str, source: &str) -> i32 {
    CompileTest::new()
        .compile_and_run(name, source)
        .unwrap_or_else(|e| panic!("{name} should compile and run: {e}"))
}

#[test]
fn a_coerced_tensor_literal_compiles_and_runs() {
    let source = r#"
func main() -> i32 {
    val v: Tensor<f32, [3]> = [1.0, 2.0, 3.0]
    val m: Tensor<f32, [2, 3]> = [
        [1.0, 2.0, 3.0],
        [4.0, 5.0, 6.0]
    ]
    val cube: Tensor<i32, [2, 2, 2]> = [
        [[1, 2], [3, 4]],
        [[5, 6], [7, 8]]
    ]
    return 7
}
"#;
    assert_eq!(run_program("tensor_literal.nr", source), 7);
}

#[test]
fn every_construction_helper_compiles_and_runs() {
    let source = r#"
func main() -> i32 {
    val z = Tensor::<f32, [3, 3]>::zeros()
    val o = Tensor::<f32, [3, 3]>::ones()
    val eye = Tensor::<f32, [4, 4]>::identity()
    val w = Tensor::<f32, [16, 8]>::random_normal(mean: 0.0f32, std: 0.02f32)
    val s: Tensor<f32, []> = Tensor::scalar(42.0)
    val v = Tensor::<f32, [3]>::from([1.0, 2.0, 3.0])
    return 3
}
"#;
    assert_eq!(run_program("tensor_ctors.nr", source), 3);
}

/// A tensor is a first-class value: it passes to a function, comes back from one, and
/// sits in a struct field, all by value.
#[test]
fn a_tensor_crosses_function_and_struct_boundaries() {
    let source = r#"
struct Layer {
    weights: Tensor<f32, [2, 2]>
}

func identity_layer() -> Layer {
    return Layer { weights: Tensor::<f32, [2, 2]>::identity() }
}

func consume(t: Tensor<f32, [2]>) -> i32 {
    return 5
}

func main() -> i32 {
    val layer = identity_layer()
    val row: Tensor<f32, [2]> = [1.0, 2.0]
    return consume(row)
}
"#;
    assert_eq!(run_program("tensor_boundaries.nr", source), 5);
}

/// The counter-example: without an annotation the literal stays a plain array, so
/// its elements are still `f64` and it is still indexable as an array.
#[test]
fn an_unannotated_literal_is_still_a_plain_array() {
    let source = r#"
func main() -> i32 {
    val arr = [1.0, 2.0, 3.0]
    return arr[2] as i32
}
"#;
    assert_eq!(run_program("tensor_vs_array.nr", source), 3);
}

/// The generator is seeded from a fixed constant, so a compiled program draws the same
/// weights on every run — a property a training script can rely on.
#[test]
fn random_normal_is_reproducible_across_runs() {
    let source = r#"
func main() -> i32 {
    val w = Tensor::<f64, [4, 4]>::random_normal(0.0, 1.0)
    return 11
}
"#;
    let test = CompileTest::new();
    let path = test.write_source("tensor_rng.nr", source);
    let binary = test.compile(&path).expect("compiles");
    let first = test.run_executable(&binary).expect("runs");
    let second = test.run_executable(&binary).expect("runs again");
    assert_eq!(first, second);
    assert_eq!(first, 11);
}

/// BUG-018: a tensor is a first-class LLVM aggregate, and `-O 0` cannot lower a copy of a
/// very large one. The limit must be a diagnostic naming the workaround, never a crash —
/// and the same program must compile at `-O 1`, where the copy becomes a memcpy.
#[test]
fn an_oversized_tensor_reports_the_limit_at_o0_and_compiles_at_o1() {
    let source = r#"
func main() -> i32 {
    val w = Tensor::<f32, [784, 128]>::random_normal(mean: 0.0f32, std: 0.02f32)
    return 0
}
"#;
    let test = CompileTest::new();
    let path = test.write_source("tensor_oversized.nr", source);

    let at_o0 = compile_at(&path, "0");
    let message = at_o0.expect_err("`-O 0` cannot lower a buffer this large");
    assert!(
        message.contains("100352 elements") && message.contains("`-O 1`"),
        "the diagnostic should name the size and the workaround; got: {message}"
    );

    compile_at(&path, "1").expect("`-O 1` lowers the copy as a memcpy");
}

/// Compile `path` at one optimization level, returning the compiler's combined output on
/// failure. The shared `CompileTest::compile` always uses the default level.
fn compile_at(path: &std::path::Path, level: &str) -> Result<(), String> {
    let output = std::process::Command::new(std::path::PathBuf::from(env!("CARGO_BIN_EXE_neurc")))
        .arg("compile")
        .arg(path)
        .arg("-o")
        .arg(path.with_extension(format!("out{level}")))
        .arg("-O")
        .arg(level)
        .output()
        .expect("failed to execute neurc");
    if output.status.success() {
        return Ok(());
    }
    let mut message = String::from_utf8_lossy(&output.stdout).into_owned();
    message.push_str(&String::from_utf8_lossy(&output.stderr));
    Err(message)
}
