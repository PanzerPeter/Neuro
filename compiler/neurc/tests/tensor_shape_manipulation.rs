// Tensor shape manipulation (Phase 2B): `.t()`, `.reshape(...)`, `.permute(...)` and
// `.flatten(...)`, end to end through `neurc check` and `neurc compile`.
//
// Every runtime assertion reads an element back out of the reshaped tensor, because the
// result's shape alone would pass even if the buffer were left in the receiver's order.
// A transpose that only relabelled the axes would put the wrong number at `t[0, 1]`.
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
fn a_transpose_moves_the_elements() {
    let source = r#"
func main() -> i32 {
    val m: Tensor<i32, [2, 3]> = [
        [1, 2, 3],
        [4, 5, 6]
    ]
    val t = m.t()
    // t is [3, 2]: rows (1, 4), (2, 5), (3, 6).
    return t[0, 1] * 50 + t[2, 0] * 10 + t[1, 1]
}
"#;
    assert_eq!(run_program("tensor_transpose.nr", source), 235);
}

#[test]
fn a_reshape_keeps_the_row_major_order() {
    let source = r#"
func main() -> i32 {
    val m: Tensor<i32, [2, 3]> = [
        [1, 2, 3],
        [4, 5, 6]
    ]
    val flat: Tensor<i32, [6]> = m.reshape([-1])
    return flat[0] * 100 + flat[3] * 10 + flat[5]
}
"#;
    assert_eq!(run_program("tensor_reshape_flat.nr", source), 146);
}

#[test]
fn a_reshape_row_is_read_at_the_new_shape() {
    let source = r#"
func main() -> i32 {
    val m: Tensor<i32, [2, 6]> = [
        [1, 2, 3, 4, 5, 6],
        [7, 8, 9, 10, 11, 12]
    ]
    val r = m.reshape([3, -1])
    // r is [3, 4]: rows (1..4), (5..8), (9..12).
    return r[1, 0] * 10 + r[2, 3]
}
"#;
    assert_eq!(run_program("tensor_reshape_infer.nr", source), 62);
}

/// The named spelling: dimension names in place of positional indices.
#[test]
fn a_permute_reorders_axes_by_dimension_name() {
    let source = r#"
func main() -> i32 {
    val image: Tensor<i32, [channels: 2, height: 2, width: 3]> = [
        [[1, 2, 3], [4, 5, 6]],
        [[7, 8, 9], [10, 11, 12]]
    ]
    val hwc = image.permute([height, width, channels])
    // hwc is [height: 2, width: 3, channels: 2].
    return hwc[0, 0, 0] * 100 + hwc[0, 0, 1] * 10 + hwc[1, 2, 1]
}
"#;
    assert_eq!(run_program("tensor_permute_named.nr", source), 182);
}

#[test]
fn a_permute_also_takes_positional_axes() {
    let source = r#"
func main() -> i32 {
    val t: Tensor<i32, [2, 2, 3]> = [
        [[1, 2, 3], [4, 5, 6]],
        [[7, 8, 9], [10, 11, 12]]
    ]
    val p = t.permute([1, 2, 0])
    return p[0, 0, 1] * 10 + p[1, 2, 0]
}
"#;
    assert_eq!(run_program("tensor_permute_positional.nr", source), 76);
}

#[test]
fn a_named_flatten_merges_a_run_of_axes() {
    let source = r#"
func main() -> i32 {
    val batch: Tensor<i32, [batch: 2, seq_len: 2, embed: 3]> = [
        [[1, 1, 1], [2, 2, 2]],
        [[3, 3, 3], [4, 4, 4]]
    ]
    val flat = batch.flatten(dims: [seq_len, embed])
    // flat is [batch: 2, 6].
    return flat[0, 5] * 10 + flat[1, 5]
}
"#;
    assert_eq!(run_program("tensor_flatten_named.nr", source), 24);
}

#[test]
fn a_bare_flatten_merges_every_axis() {
    let source = r#"
func main() -> i32 {
    val t: Tensor<i32, [2, 2]> = [
        [1, 2],
        [3, 4]
    ]
    val flat = t.flatten()
    return flat[1] * 100 + flat[2] * 10 + flat[3]
}
"#;
    assert_eq!(run_program("tensor_flatten_all.nr", source), 234);
}

/// A shape cast consumes its receiver, so the transposed matrix and the original cannot
/// both be live.
#[test]
fn a_shape_method_moves_its_receiver() {
    let source = r#"
func main() -> i32 {
    val m: Tensor<i32, [2, 2]> = Tensor::<i32, [2, 2]>::ones()
    val t = m.t()
    return m[0, 0]
}
"#;
    let out = rejection("tensor_shape_move.nr", source);
    assert!(
        out.contains("moved"),
        "the receiver should be moved; got: {out}"
    );
}

/// `.clone()` is the documented way to keep the receiver, and it still works through a
/// shape cast: the clone is transposed and the original stays readable.
#[test]
fn a_clone_keeps_the_receiver_alive_across_a_transpose() {
    let source = r#"
func main() -> i32 {
    val m: Tensor<i32, [2, 3]> = [
        [1, 2, 3],
        [4, 5, 6]
    ]
    val t = m.clone().t()
    return t[2, 1] * 10 + m[0, 2]
}
"#;
    assert_eq!(run_program("tensor_shape_clone.nr", source), 63);
}

#[test]
fn an_unknown_dimension_name_lists_the_declared_ones() {
    let source = r#"
func main() -> i32 {
    val image: Tensor<i32, [channels: 2, height: 2, width: 3]> = Tensor::<i32, [2, 2, 3]>::ones()
    val p = image.permute([height, depth, channels])
    return 0
}
"#;
    let out = rejection("tensor_permute_unknown.nr", source);
    assert!(
        out.contains("depth") && out.contains("channels") && out.contains("width"),
        "the diagnostic should list the declared names; got: {out}"
    );
}

#[test]
fn a_reshape_that_changes_the_element_count_is_rejected() {
    let source = r#"
func main() -> i32 {
    val m: Tensor<i32, [2, 3]> = Tensor::<i32, [2, 3]>::ones()
    val r = m.reshape([4, 2])
    return 0
}
"#;
    let out = rejection("tensor_reshape_count.nr", source);
    assert!(
        out.contains("8") && out.contains("6"),
        "the diagnostic should name both counts; got: {out}"
    );
}

/// A transposed named shape is not the shape it came from, which is the transposition
/// error named dimensions exist to catch.
#[test]
fn a_transposed_named_shape_is_rejected_where_the_original_is_expected() {
    let source = r#"
func normalize(x: Tensor<f32, [height: 4, width: 4]>) { }

func main() -> i32 {
    val t: Tensor<f32, [height: 4, width: 4]> = Tensor::<f32, [4, 4]>::zeros()
    normalize(t.t())
    return 0
}
"#;
    let out = rejection("tensor_transpose_names.nr", source);
    assert!(
        out.contains("height") && out.contains("width"),
        "the diagnostic should name the disagreeing axis; got: {out}"
    );
}

/// A shape cast composes with the rest of the tensor surface: a permuted tensor is
/// sliced, compound-assigned, and read back.
#[test]
fn a_permuted_tensor_still_slices_and_updates_in_place() {
    let source = r#"
func main() -> i32 {
    val image: Tensor<i32, [channels: 2, height: 2, width: 3]> = [
        [[1, 2, 3], [4, 5, 6]],
        [[7, 8, 9], [10, 11, 12]]
    ]
    mut hwc = image.permute([height, width, channels])
    hwc += Tensor::<i32, [2, 3, 2]>::ones()
    val row = hwc[0, .., ..]
    return row[2, 1]
}
"#;
    assert_eq!(run_program("tensor_permute_compose.nr", source), 10);
}
