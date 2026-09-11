use super::semantic_errors;
use crate::errors::TypeError;

/// A shape-manipulation call is well-typed when it names the result's own shape, so the
/// four forms are exercised together against annotations that only match if each one
/// computed the shape the specification gives it.
#[test]
fn the_four_shape_methods_produce_their_specified_shapes() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val m: Tensor<i32, [2, 3]> = Tensor::<i32, [2, 3]>::ones()
    val t: Tensor<i32, [3, 2]> = m.t()

    val a: Tensor<i32, [2, 3]> = Tensor::<i32, [2, 3]>::ones()
    val flat: Tensor<i32, [6]> = a.reshape([-1])

    val b: Tensor<i32, [2, 3, 4]> = Tensor::<i32, [2, 3, 4]>::ones()
    val p: Tensor<i32, [4, 2, 3]> = b.permute([2, 0, 1])

    val c: Tensor<i32, [2, 3, 4]> = Tensor::<i32, [2, 3, 4]>::ones()
    val f: Tensor<i32, [2, 12]> = c.flatten(dims: [1, 2])

    val d: Tensor<i32, [2, 3]> = Tensor::<i32, [2, 3]>::ones()
    val all: Tensor<i32, [6]> = d.flatten()
    return 0
}
"#,
    );
    assert!(
        errors.is_empty(),
        "shape methods should check; got {errors:?}"
    );
}

/// `-1` takes the extent the others leave over, which is the documented form.
#[test]
fn reshape_infers_the_single_negative_extent() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val m: Tensor<i32, [3, 4]> = Tensor::<i32, [3, 4]>::ones()
    val r: Tensor<i32, [2, 6]> = m.reshape([2, -1])
    return 0
}
"#,
    );
    assert!(errors.is_empty(), "`-1` should be inferred; got {errors:?}");
}

#[test]
fn reshape_rejects_an_element_count_change() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val m: Tensor<i32, [2, 3]> = Tensor::<i32, [2, 3]>::ones()
    val r = m.reshape([4, 2])
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::TensorReshapeElementCount { .. })),
        "a reshape may not change the element count; got {errors:?}"
    );
}

#[test]
fn reshape_rejects_a_second_inferred_extent() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val m: Tensor<i32, [2, 3]> = Tensor::<i32, [2, 3]>::ones()
    val r = m.reshape([-1, -1])
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::TensorReshapeRepeatedInference { .. })),
        "only one extent can be inferred; got {errors:?}"
    );
}

#[test]
fn transpose_rejects_a_rank_other_than_two() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val m: Tensor<i32, [2, 2, 2]> = Tensor::<i32, [2, 2, 2]>::ones()
    val t = m.t()
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::TensorTransposeRank { rank: 3, .. })),
        "`.t()` is the matrix transpose; got {errors:?}"
    );
}

/// The rule this item exists to make reachable: a dimension name is resolved
/// against the receiver's own shape, and an unknown one lists the names that shape does
/// declare.
#[test]
fn permute_resolves_dimension_names_and_lists_them_when_unknown() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val image: Tensor<i32, [channels: 2, height: 3, width: 4]> = Tensor::<i32, [2, 3, 4]>::ones()
    val p = image.permute([height, depth, channels])
    return 0
}
"#,
    );
    let listed = errors.iter().find_map(|e| match e {
        TypeError::UnknownTensorAxisName { name, declared, .. } => {
            Some((name.clone(), declared.clone()))
        }
        _ => None,
    });
    let (name, declared) =
        listed.unwrap_or_else(|| panic!("an unknown name should be reported; got {errors:?}"));
    assert_eq!(name, "depth");
    assert!(
        declared.contains("channels") && declared.contains("height") && declared.contains("width"),
        "the diagnostic should list the declared names; got '{declared}'"
    );
}

/// A local binding of the same name neither shadows the axis nor is shadowed by it:
/// names live in a namespace attached to the tensor type.
#[test]
fn a_dimension_name_is_not_looked_up_in_the_value_scope() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val height: string = "not an axis"
    val image: Tensor<i32, [channels: 2, height: 3, width: 4]> = Tensor::<i32, [2, 3, 4]>::ones()
    val p: Tensor<i32, [3, 4, 2]> = image.permute([height, width, channels])
    return 0
}
"#,
    );
    assert!(
        errors.is_empty(),
        "a dimension name resolves against the shape; got {errors:?}"
    );
}

#[test]
fn permute_rejects_a_repeated_axis() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val m: Tensor<i32, [2, 3]> = Tensor::<i32, [2, 3]>::ones()
    val p = m.permute([0, 0])
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::TensorAxisRepeated { .. })),
        "a permutation names each axis once; got {errors:?}"
    );
}

#[test]
fn flatten_rejects_non_adjacent_axes() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val t: Tensor<i32, [a: 2, b: 3, c: 4]> = Tensor::<i32, [2, 3, 4]>::ones()
    val f = t.flatten(dims: [a, c])
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::TensorFlattenNotAdjacent { .. })),
        "flattening merges a contiguous run; got {errors:?}"
    );
}

/// The receiver is consumed, so the transposed matrix and the original cannot both be
/// live: a transpose must not leave a second buffer alive behind the programmer's back.
#[test]
fn a_shape_method_moves_its_receiver() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val m: Tensor<i32, [2, 3]> = Tensor::<i32, [2, 3]>::ones()
    val t = m.t()
    return m[0, 0]
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::UseOfMovedValue { .. })),
        "the receiver is moved; got {errors:?}"
    );
}

/// Consuming needs an owned receiver, so a borrow falls through to the ordinary method
/// surface and is reported as a missing method rather than moving out of someone else's
/// tensor. The same rule `.to(device)` already follows.
#[test]
fn a_borrowed_receiver_has_no_shape_method() {
    let errors = semantic_errors(
        r#"
func flip(m: &Tensor<i32, [2, 3]>) -> i32 {
    val t = m.t()
    return 0
}

func main() -> i32 {
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::MethodNotFound { .. })),
        "a borrow cannot be consumed; got {errors:?}"
    );
}

/// A transposed shape keeps its names, in the transposed order — which is exactly the
/// transposition error named dimensions exist to catch.
#[test]
fn transposing_a_named_shape_transposes_its_names() {
    let errors = semantic_errors(
        r#"
func normalize(x: Tensor<f32, [height: 4, width: 4]>) { }

func main() -> i32 {
    val t: Tensor<f32, [height: 4, width: 4]> = Tensor::<f32, [4, 4]>::zeros()
    normalize(t.t())
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::TensorAxisNameMismatch { .. })),
        "the transposed names should disagree; got {errors:?}"
    );
}

/// An extent that is a shape parameter is not known until the instantiation, so a
/// reshape written inside a shape-generic function has no element count to check.
#[test]
fn a_shape_parameter_extent_is_rejected_by_reshape() {
    let errors = semantic_errors(
        r#"
func flatten_rows<N>(t: Tensor<i32, [N, 2]>) -> i32 {
    val f = t.reshape([-1])
    return 0
}

func main() -> i32 {
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::TensorShapeCastSymbolicExtent { .. })),
        "a symbolic extent has no element count; got {errors:?}"
    );
}
