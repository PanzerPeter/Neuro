// Tensor reductions (Phase 2B): `.sum()`, `.mean()`, `.max()` and `.min()`, whole-tensor
// and along one axis, end to end through `neurc check` and `neurc compile`.
//
// Every runtime assertion reads the reduced values back, because a result of the right
// SHAPE would pass even if the traversal gathered the wrong run: a column sum computed
// over rows has the same type as the column sum.
//
// An exit code is one byte, so each expected value is kept below 256.
mod common;

use common::CompileTest;

/// Compile and run `source`, returning its exit code.
fn run_program(name: &str, source: &str) -> i32 {
    CompileTest::new()
        .compile_and_run(name, source)
        .unwrap_or_else(|e| panic!("{name} should compile and run: {e}"))
}

/// The diagnostics from a program that must not check.
fn rejection(name: &str, source: &str) -> String {
    CompileTest::new()
        .check(name, source)
        .expect_err(&format!("{name} should be rejected"))
}

#[test]
fn a_whole_tensor_reduction_folds_every_element() {
    let source = r#"
func main() -> i32 {
    val m: Tensor<i32, [2, 3]> = [
        [1, 2, 3],
        [4, 5, 6]
    ]
    // 21 + 6 * 10 + 1 * 100 = 181
    return m.sum() + m.max() * 10 + m.min() * 100
}
"#;
    assert_eq!(run_program("tensor_reduce_whole.nr", source), 181);
}

/// The two axes of the same matrix reduce over different runs, which is what a
/// shape-only assertion would miss.
#[test]
fn an_axis_reduction_gathers_that_axis_run() {
    let source = r#"
func main() -> i32 {
    val m: Tensor<i32, [2, 3]> = [
        [1, 2, 3],
        [4, 5, 6]
    ]
    val rows: Tensor<i32, [2]> = m.sum(axis: 1)
    val cols: Tensor<i32, [3]> = m.sum(axis: 0)
    // rows = (6, 15), cols = (5, 7, 9).
    return rows[0] * 10 + rows[1] + cols[0] + cols[2] * 10
}
"#;
    assert_eq!(run_program("tensor_reduce_axis.nr", source), 170);
}

#[test]
fn a_reduction_of_a_rank_three_tensor_keeps_the_surviving_axes() {
    let source = r#"
func main() -> i32 {
    val t: Tensor<i32, [2, 2, 2]> = [
        [[1, 2], [3, 4]],
        [[5, 6], [7, 8]]
    ]
    val middle: Tensor<i32, [2, 2]> = t.sum(axis: 1)
    // middle = ((1+3, 2+4), (5+7, 6+8)) = ((4, 6), (12, 14)).
    return middle[0, 0] * 10 + middle[0, 1] + middle[1, 0] + middle[1, 1]
}
"#;
    assert_eq!(run_program("tensor_reduce_rank3.nr", source), 72);
}

#[test]
fn a_mean_divides_by_the_reduced_run_length() {
    let source = r#"
func main() -> i32 {
    val m: Tensor<f64, [2, 3]> = [
        [1.0, 2.0, 3.0],
        [7.0, 8.0, 9.0]
    ]
    val all: f64 = m.mean()
    val rows: Tensor<f64, [2]> = m.mean(axis: 1)
    // all = 5.0, rows = (2.0, 8.0).
    return (all as i32) * 10 + (rows[0] as i32) + (rows[1] as i32)
}
"#;
    assert_eq!(run_program("tensor_reduce_mean.nr", source), 60);
}

/// A negative axis counts from the end, and a dimension name selects the same axis its
/// position does.
#[test]
fn an_axis_may_be_named_or_counted_from_the_end() {
    let source = r#"
func main() -> i32 {
    val m: Tensor<i32, [height: 2, width: 3]> = [
        [1, 2, 3],
        [4, 5, 6]
    ]
    val by_name: Tensor<i32, [width: 3]> = m.max(axis: height)
    val by_index: Tensor<i32, [height: 2]> = m.max(axis: -1)
    // by_name = (4, 5, 6), by_index = (3, 6).
    return by_name[2] * 10 + by_index[0] * 5 + by_index[1]
}
"#;
    assert_eq!(run_program("tensor_reduce_named_axis.nr", source), 81);
}

/// A reduction reads its receiver: the tensor is still there afterwards, and a borrow is
/// a legal receiver, which is what lets a weight be summarised without moving it.
#[test]
fn a_reduction_leaves_its_receiver_alive() {
    let source = r#"
func spread(t: &Tensor<i32, [4]>) -> i32 {
    t.max() - t.min()
}

func main() -> i32 {
    val v: Tensor<i32, [4]> = [4, 9, 2, 7]
    val range: i32 = spread(&v)
    val total: i32 = v.sum()
    return range * 10 + total
}
"#;
    assert_eq!(run_program("tensor_reduce_borrowed.nr", source), 92);
}

/// Reducing a vector along its only axis yields the rank-0 tensor. Indexing cannot read
/// one back — a rank-0 index has no argument to give — so the value is reached by
/// reducing it again, over the empty product of extents that is its single element.
#[test]
fn reducing_a_vector_yields_a_rank_zero_tensor() {
    let source = r#"
func main() -> i32 {
    val v: Tensor<i32, [4]> = [4, 9, 2, 7]
    val folded: Tensor<i32, []> = v.sum(axis: 0)
    return folded.sum()
}
"#;
    assert_eq!(run_program("tensor_reduce_rank0.nr", source), 22);
}

#[test]
fn a_non_numeric_element_is_rejected() {
    let source = r#"
func main() -> i32 {
    val b: Tensor<bool, [2]> = [true, false]
    val x: bool = b.max()
    return 0
}
"#;
    let errors = rejection("tensor_reduce_bool.nr", source);
    assert!(
        errors.contains("integer or `f32`/`f64` element type"),
        "a bool tensor should be rejected: {errors}"
    );
}

#[test]
fn an_integer_mean_is_rejected() {
    let source = r#"
func main() -> i32 {
    val v: Tensor<i32, [2]> = [1, 2]
    val x: i32 = v.mean()
    return 0
}
"#;
    let errors = rejection("tensor_reduce_int_mean.nr", source);
    assert!(
        errors.contains("no rounding rule"),
        "an integer mean should be rejected: {errors}"
    );
}

#[test]
fn a_reduction_over_no_elements_is_rejected() {
    let source = r#"
func main() -> i32 {
    val v: Tensor<i32, [0]> = []
    val x: i32 = v.min()
    return 0
}
"#;
    let errors = rejection("tensor_reduce_empty.nr", source);
    assert!(
        errors.contains("reduces over no elements"),
        "an empty reduction should be rejected: {errors}"
    );
}

#[test]
fn an_axis_outside_the_rank_is_rejected() {
    let source = r#"
func main() -> i32 {
    val m: Tensor<i32, [2, 2]> = [[1, 2], [3, 4]]
    val x: Tensor<i32, [2]> = m.sum(axis: 3)
    return 0
}
"#;
    let errors = rejection("tensor_reduce_bad_axis.nr", source);
    assert!(
        errors.contains("out of range"),
        "an out-of-range axis should be rejected: {errors}"
    );
}

#[test]
fn an_unknown_dimension_name_is_rejected() {
    let source = r#"
func main() -> i32 {
    val m: Tensor<i32, [rows: 2, cols: 2]> = [[1, 2], [3, 4]]
    val x: Tensor<i32, [2]> = m.sum(axis: depth)
    return 0
}
"#;
    let errors = rejection("tensor_reduce_bad_name.nr", source);
    assert!(
        errors.contains("no dimension named 'depth'"),
        "an unknown dimension should be rejected: {errors}"
    );
}
