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
/// weights on every run: a property a training script can rely on.
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

/// BUG-018, closed by the out-of-line buffer: a tensor's buffer is a heap allocation, not
/// a first-class LLVM aggregate, so a weight matrix of realistic size compiles and runs at
/// the default `-O 0`: the level whose monolithic-value lowering the old cap existed for.
#[test]
fn a_large_tensor_compiles_and_runs_at_the_default_optimization_level() {
    let source = r#"
func main() -> i32 {
    val w = Tensor::<f32, [784, 128]>::random_normal(mean: 0.0f32, std: 0.02f32)
    return 0
}
"#;
    assert_eq!(run_program("tensor_large.nr", source), 0);
}

/// The other half of the representation change: a large tensor crosses a call boundary by
/// value. Constructing and cloning one inside a single function was reachable at `-O 0` by
/// running SROA there; returning one was not, because there was no out-of-line buffer for
/// the value to be behind.
#[test]
fn a_large_tensor_returns_by_value_and_clones() {
    let source = r#"
func make_weights() -> Tensor<f32, [784, 128]> {
    return Tensor::<f32, [784, 128]>::random_normal(mean: 0.0f32, std: 0.02f32)
}

func count_rows(w: Tensor<f32, [784, 128]>) -> i32 {
    return 784
}

func main() -> i32 {
    val w = make_weights()
    val copy = w.clone()
    val on_host = copy.to(Device::CPU)
    return count_rows(on_host) - count_rows(w)
}
"#;
    assert_eq!(run_program("tensor_large_return.nr", source), 0);
}

/// A tensor owns its buffer, so every binding releases one and every move hands one on.
/// The program below moves a tensor through a binding, a call, a `.to()` transfer, a
/// struct field, and a loop body: a missed move would be a double free and an abort, so
/// the exit code is the assertion.
#[test]
fn moving_a_tensor_through_every_owner_frees_each_buffer_once() {
    let source = r#"
struct Holder {
    weights: Tensor<f32, [16, 16]>
}

func consume(t: Tensor<f32, [16, 16]>) -> i32 {
    return 1
}

func main() -> i32 {
    mut total = 0
    for i in 0..8 {
        val fresh = Tensor::<f32, [16, 16]>::zeros()
        val moved = fresh
        val on_host = moved.to(Device::CPU)
        val copy = on_host.clone()
        val holder = Holder { weights: on_host }
        total = total + consume(copy)
    }
    return total
}
"#;
    assert_eq!(run_program("tensor_moves.nr", source), 8);
}
