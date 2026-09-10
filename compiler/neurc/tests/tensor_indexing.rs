// Tensor indexing and slicing (Phase 2B): `t[i, j]`, `t[0, ..]`, `t[1..3, 2..5]`
// and `t[0..=2, ..]`, end to end through `neurc compile` and the linked binary.
//
// Every assertion is on a VALUE read back out of a tensor, which is what this feature
// makes possible for the first time: before it, a tensor could only be asserted on
// through its shape and its parameter count.
mod common;

use common::CompileTest;

/// Compile and run `source`, returning its exit code.
fn run_program(name: &str, source: &str) -> i32 {
    CompileTest::new()
        .compile_and_run(name, source)
        .unwrap_or_else(|e| panic!("{name} should compile and run: {e}"))
}

#[test]
fn an_element_is_read_by_naming_every_axis() {
    let source = r#"
func main() -> i32 {
    val m: Tensor<i32, [3, 4]> = [
        [0, 1, 2, 3],
        [10, 11, 12, 13],
        [20, 21, 22, 23]
    ]
    return m[2, 1]
}
"#;
    assert_eq!(run_program("tensor_element.nr", source), 21);
}

/// A run-time position is legal on every axis, so a loop can walk a tensor.
#[test]
fn a_runtime_position_indexes_inside_a_loop() {
    let source = r#"
func main() -> i32 {
    val m: Tensor<i32, [3, 3]> = [
        [1, 2, 3],
        [4, 5, 6],
        [7, 8, 9]
    ]
    mut trace = 0
    for i in 0..3 {
        trace = trace + m[i, i]
    }
    return trace
}
"#;
    assert_eq!(run_program("tensor_trace.nr", source), 15);
}

/// `matrix[0, ..]` and `matrix[.., 1]`: an axis given a position is dropped, so
/// a row and a column of a matrix are both rank-1 tensors.
#[test]
fn a_full_axis_slice_reads_a_row_and_a_column() {
    let source = r#"
func main() -> i32 {
    val m: Tensor<i32, [3, 4]> = [
        [0, 1, 2, 3],
        [10, 11, 12, 13],
        [20, 21, 22, 23]
    ]
    val row: Tensor<i32, [4]> = m[1, ..]
    val column: Tensor<i32, [3]> = m[.., 2]
    return row[3] + column[0] + column[2]
}
"#;
    assert_eq!(run_program("tensor_axis_slice.nr", source), 13 + 2 + 22);
}

/// `matrix[1..3, 2..5]` and `matrix[0..=2, 0..=2]`, on one tensor.
#[test]
fn a_ranged_slice_keeps_its_axes_at_the_new_extents() {
    let source = r#"
func main() -> i32 {
    val m: Tensor<i32, [4, 5]> = [
        [0, 1, 2, 3, 4],
        [10, 11, 12, 13, 14],
        [20, 21, 22, 23, 24],
        [30, 31, 32, 33, 34]
    ]
    val exclusive: Tensor<i32, [2, 3]> = m[1..3, 2..5]
    val inclusive: Tensor<i32, [3, 3]> = m[0..=2, 0..=2]
    return exclusive[0, 0] + exclusive[1, 2] + inclusive[2, 2]
}
"#;
    assert_eq!(run_program("tensor_range_slice.nr", source), 12 + 24 + 22);
}

/// `image[.., 10..42, 10..42]`: a rank-3 tensor sliced on two axes while the
/// third is taken whole.
#[test]
fn a_rank_three_patch_combines_every_axis_form() {
    let source = r#"
func main() -> i32 {
    val cube: Tensor<i32, [2, 3, 4]> = [
        [[0, 1, 2, 3], [4, 5, 6, 7], [8, 9, 10, 11]],
        [[12, 13, 14, 15], [16, 17, 18, 19], [20, 21, 22, 23]]
    ]
    val patch: Tensor<i32, [2, 2, 2]> = cube[.., 1..3, 1..=2]
    val plane: Tensor<i32, [3, 4]> = cube[1, .., ..]
    return patch[0, 0, 0] + patch[1, 1, 1] + plane[2, 3]
}
"#;
    assert_eq!(run_program("tensor_patch.nr", source), 5 + 22 + 23);
}

/// A slice is an owned tensor of its own, so it may be sliced again, moved into a
/// function, and cloned like any other.
#[test]
fn a_slice_is_an_owned_tensor_that_can_be_sliced_again() {
    let source = r#"
func total(t: Tensor<i32, [2]>) -> i32 {
    return t[0] + t[1]
}

func main() -> i32 {
    val m: Tensor<i32, [3, 3]> = [
        [1, 2, 3],
        [4, 5, 6],
        [7, 8, 9]
    ]
    val block = m[1..3, 0..2]
    val row = block[1, ..]
    val copy = row.clone()
    return total(row) + total(copy)
}
"#;
    assert_eq!(run_program("tensor_slice_owned.nr", source), 2 * 15);
}

/// Indexing reads through a borrow: the caller still owns the tensor afterwards, which
/// is what lets one weight be inspected repeatedly.
#[test]
fn a_borrowed_tensor_is_indexed_without_being_consumed() {
    let source = r#"
func corner(t: &Tensor<i32, [2, 2]>) -> i32 {
    return t[1, 1]
}

func first_row(t: &Tensor<i32, [2, 2]>) -> Tensor<i32, [2]> {
    return t[0, ..]
}

func main() -> i32 {
    val m: Tensor<i32, [2, 2]> = [[1, 2], [3, 4]]
    val a = corner(&m)
    val row = first_row(&m)
    val b = corner(&m)
    return a + b + row[1]
}
"#;
    assert_eq!(run_program("tensor_borrow_index.nr", source), 4 + 4 + 2);
}

/// A slice of a float tensor keeps its element type and its values.
#[test]
fn a_float_tensor_slices_and_reads_back() {
    let source = r#"
func main() -> i32 {
    val m: Tensor<f32, [2, 3]> = [
        [0.5, 1.5, 2.5],
        [3.5, 4.5, 5.5]
    ]
    val row: Tensor<f32, [3]> = m[1, ..]
    val doubled = row[2] * 2.0f32
    return doubled as i32
}
"#;
    assert_eq!(run_program("tensor_float_slice.nr", source), 11);
}

/// The in-place update and the index meet on one buffer: `w -= g` writes through the
/// handle, and the index reads what it wrote.
#[test]
fn an_index_reads_what_a_compound_assignment_wrote() {
    let source = r#"
func main() -> i32 {
    mut w: Tensor<i32, [2, 2]> = [[10, 20], [30, 40]]
    val g: Tensor<i32, [2, 2]> = [[1, 2], [3, 4]]
    w -= &g
    w -= &g
    return w[0, 0] + w[1, 1]
}
"#;
    assert_eq!(run_program("tensor_index_after_update.nr", source), 8 + 32);
}

/// A position outside its axis panics on the debug tier, the tier an array's bounds
/// check sits on.
///
/// `run_executable` reports an abort delivered as a signal (Unix `SIGABRT`, where there
/// is no exit code at all) as `-1`, and Windows hands back the panic runtime's NTSTATUS
/// exception code as a negative `i32`, so a negative code means aborted on both.
#[test]
fn a_runtime_position_out_of_bounds_aborts_in_a_debug_build() {
    let source = r#"
func main() -> i32 {
    val m: Tensor<i32, [2, 3]> = [[1, 2, 3], [4, 5, 6]]
    mut acc = 0
    for i in 0..4 {
        acc = acc + m[i, 0]
    }
    return acc
}
"#;
    let exit_code = run_program("tensor_index_oob.nr", source);
    assert!(
        exit_code < 0,
        "an out-of-range tensor index aborts; got {exit_code}"
    );
}
