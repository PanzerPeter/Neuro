// In-place compound assignment on tensors (Phase 2B): `w += g`, `w -= g`, and the
// rest of the `*Assign` family, end to end through `neurc compile` and the linked binary.
//
// A tensor still cannot be read back — indexing and the reductions are later 2B items —
// so element values are asserted with the guard the elements already carry. Adding a
// value to `i32`'s maximum panics on the debug tier exactly when that value is positive,
// and adding it to the minimum panics exactly when it is negative, so bracketing a
// difference between the two proves every element of it is zero. A wrong element aborts
// the program instead of returning its exit code.
mod common;

use common::CompileTest;

fn run_program(name: &str, source: &str) -> i32 {
    CompileTest::new()
        .compile_and_run(name, source)
        .unwrap_or_else(|e| panic!("{name} should compile and run: {e}"))
}

fn compile_and_run(name: &str, source: &str) -> Result<i32, String> {
    CompileTest::new().compile_and_run(name, source)
}

/// The exit code a program aborted by a panicking guard leaves behind.
const ABORTED: i32 = -1;

/// Every arithmetic operator has a tensor form, and each one computes the element-wise
/// result: the bracket at the end holds only if `w` matched its expectation exactly.
#[test]
fn every_compound_operator_updates_elements_in_place() {
    let source = r#"
func assert_zero(diff: &Tensor<i32, [4]>) {
    mut hi: Tensor<i32, [4]> = [2147483647, 2147483647, 2147483647, 2147483647]
    hi += diff
    mut lo: Tensor<i32, [4]> = [-2147483648, -2147483648, -2147483648, -2147483648]
    lo += diff
}

func main() -> i32 {
    mut w: Tensor<i32, [4]> = [10, 20, 30, 40]
    val ones: Tensor<i32, [4]> = [1, 1, 1, 1]
    val twos: Tensor<i32, [4]> = [2, 2, 2, 2]

    w += &ones              // 11, 21, 31, 41
    w -= &twos              // 9, 19, 29, 39
    w *= &twos              // 18, 38, 58, 78
    w /= &twos              // 9, 19, 29, 39
    w %= &twos              // 1, 1, 1, 1

    val expected: Tensor<i32, [4]> = [1, 1, 1, 1]
    w -= &expected
    assert_zero(&w)
    return 6
}
"#;
    assert_eq!(run_program("tensor_ops.nr", source), 6);
}

/// The same bracket, deliberately failed: a program whose expectation is one off aborts,
/// which is what makes the passing case above evidence rather than a program that merely
/// runs.
#[test]
fn a_wrong_element_is_caught_by_the_bracket() {
    let source = r#"
func main() -> i32 {
    mut w: Tensor<i32, [2]> = [10, 20]
    val g: Tensor<i32, [2]> = [1, 1]
    w += &g
    val wrong: Tensor<i32, [2]> = [11, 22]
    w -= &wrong
    mut hi: Tensor<i32, [2]> = [2147483647, 2147483647]
    hi += &w
    mut lo: Tensor<i32, [2]> = [-2147483648, -2147483648]
    lo += &w
    return 0
}
"#;
    assert_eq!(run_program("tensor_ops_wrong.nr", source), ABORTED);
}

/// Float elements take the same path; `%` on floats is the IEEE remainder the scalar
/// operator already provides.
#[test]
fn a_float_tensor_updates_in_place() {
    let source = r#"
func main() -> i32 {
    mut w: Tensor<f32, [2, 2]> = [[1.0, 2.0], [3.0, 4.0]]
    val g: Tensor<f32, [2, 2]> = [[0.5, 0.5], [0.5, 0.5]]
    w += &g
    w -= &g
    w *= &g
    w /= &g
    w %= &g
    return 8
}
"#;
    assert_eq!(run_program("tensor_ops_float.nr", source), 8);
}

/// A weight matrix at real scale, updated in place inside a loop: the shape a training
/// step has, and the case the by-value desugaring would make allocate once per step.
#[test]
fn a_large_weight_matrix_is_updated_repeatedly() {
    let source = r#"
func main() -> i32 {
    mut w = Tensor::<f32, [784, 128]>::random_normal(mean: 0.0f32, std: 0.02f32)
    val step = Tensor::<f32, [784, 128]>::zeros()
    for i in 0..8 {
        w -= &step
    }
    return 0
}
"#;
    assert_eq!(run_program("tensor_ops_large.nr", source), 0);
}

/// The element's own guards survive into the tensor form: an overflowing element panics
/// on the debug tier the way an overflowing scalar does.
#[test]
fn an_overflowing_element_panics() {
    let source = r#"
func main() -> i32 {
    mut w: Tensor<i32, [2]> = [2147483647, 0]
    val g: Tensor<i32, [2]> = [1, 0]
    w += &g
    return 0
}
"#;
    assert_eq!(run_program("tensor_ops_overflow.nr", source), ABORTED);
}

/// A zero divisor panics in every build, so an element-wise division by a zeroed tensor
/// aborts rather than raising `SIGFPE` without a diagnostic.
#[test]
fn a_zero_divisor_element_panics() {
    let source = r#"
func main() -> i32 {
    mut w: Tensor<i32, [2]> = [4, 4]
    val g = Tensor::<i32, [2]>::zeros()
    w /= &g
    return 0
}
"#;
    assert_eq!(run_program("tensor_ops_divzero.nr", source), ABORTED);
}

/// The update runs through the target's own handle, so a tensor moved into a struct field
/// and one handed across a call boundary are still the same buffer afterwards: the
/// program releases exactly one handle per tensor and exits normally.
#[test]
fn an_updated_tensor_crosses_ownership_boundaries() {
    let source = r#"
struct Layer {
    weights: Tensor<f32, [4, 4]>
}

func gradient() -> Tensor<f32, [4, 4]> {
    return Tensor::<f32, [4, 4]>::ones()
}

func main() -> i32 {
    mut w = Tensor::<f32, [4, 4]>::identity()
    w -= gradient()
    w += Tensor::<f32, [4, 4]>::ones()
    val layer = Layer { weights: w }
    return 3
}
"#;
    assert_eq!(run_program("tensor_ops_ownership.nr", source), 3);
}

/// Scalar compound assignment is unaffected by the tensor path: it still desugars to
/// `x = x OP rhs`. The user-type half of the desugaring — an operator-trait impl reached
/// through `+=` — is covered by `operator_traits.rs`.
#[test]
fn scalar_compound_assignment_still_desugars() {
    let source = r#"
func main() -> i32 {
    mut n = 10
    n += 5
    n *= 2
    n -= 4
    n /= 2
    n %= 7
    return n
}
"#;
    assert_eq!(run_program("tensor_ops_scalars.nr", source), 6);
}

/// A shape is part of the type, so a differently shaped operand is a compile error rather
/// than a truncated or overrunning update.
#[test]
fn a_mismatched_shape_is_rejected_before_codegen() {
    let source = r#"
func main() -> i32 {
    mut w: Tensor<f32, [2, 2]> = [[1.0, 2.0], [3.0, 4.0]]
    val g: Tensor<f32, [4]> = [1.0, 1.0, 1.0, 1.0]
    w += &g
    return 0
}
"#;
    let error = compile_and_run("tensor_ops_shape.nr", source)
        .expect_err("a shape mismatch should not compile");
    assert!(
        error.contains("Tensor<f32, [2, 2]>"),
        "the diagnostic names the shape it expected: {error}"
    );
}
