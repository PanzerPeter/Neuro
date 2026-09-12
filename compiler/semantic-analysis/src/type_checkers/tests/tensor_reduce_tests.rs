use super::semantic_errors;
use crate::errors::TypeError;

/// A whole-tensor reduction has the element type, and an axis reduction the receiver's
/// other axes in order. Both are asserted through annotations that only accept the type
/// the specification gives them.
#[test]
fn reductions_produce_their_specified_types() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val m: Tensor<i32, [2, 3]> = Tensor::<i32, [2, 3]>::ones()
    val total: i32 = m.sum()
    val peak: i32 = m.max()
    val low: i32 = m.min()
    val rows: Tensor<i32, [2]> = m.sum(1)
    val cols: Tensor<i32, [3]> = m.min(0)

    val f: Tensor<f64, [2, 2]> = Tensor::<f64, [2, 2]>::ones()
    val avg: f64 = f.mean()
    val col_means: Tensor<f64, [2]> = f.mean(0)
    return 0
}
"#,
    );
    assert!(errors.is_empty(), "reductions should check; got {errors:?}");
}

/// Reducing a rank-1 tensor along its only axis leaves the rank-0 tensor, which is the
/// same shape `Tensor::scalar` builds.
#[test]
fn an_axis_reduction_of_a_vector_is_rank_zero() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val v: Tensor<i32, [4]> = Tensor::<i32, [4]>::ones()
    val s: Tensor<i32, []> = v.sum(0)
    return 0
}
"#,
    );
    assert!(
        errors.is_empty(),
        "rank-0 result should check; got {errors:?}"
    );
}

/// A dimension name selects an axis, and a negative index counts from the end. The
/// surviving axes keep their names, which is what makes the annotation here accept.
#[test]
fn an_axis_may_be_named_or_counted_from_the_end() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val v: Tensor<i32, [height: 2, width: 3]> = Tensor::<i32, [2, 3]>::ones()
    val over_rows: Tensor<i32, [width: 3]> = v.sum(height)
    val over_last: Tensor<i32, [height: 2]> = v.max(-1)
    return 0
}
"#,
    );
    assert!(errors.is_empty(), "axis forms should check; got {errors:?}");
}

/// A reduction reads its receiver, so a borrow is a legal receiver and the tensor is
/// still usable afterwards.
#[test]
fn a_reduction_neither_moves_nor_needs_ownership() {
    let errors = semantic_errors(
        r#"
func total(t: &Tensor<i32, [3]>) -> i32 {
    t.sum()
}

func main() -> i32 {
    val v: Tensor<i32, [3]> = Tensor::<i32, [3]>::ones()
    val once: i32 = v.sum()
    val twice: i32 = total(&v)
    val thrice: i32 = v.max()
    return 0
}
"#,
    );
    assert!(
        errors.is_empty(),
        "a reduction should not consume its receiver; got {errors:?}"
    );
}

#[test]
fn a_bool_tensor_has_nothing_to_reduce() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val b: Tensor<bool, [2]> = [true, false]
    val x: bool = b.max()
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::TensorReduceElementType { .. })),
        "a bool element should be rejected; got {errors:?}"
    );
}

#[test]
fn an_integer_mean_has_no_rounding_rule() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val v: Tensor<i32, [2]> = Tensor::<i32, [2]>::ones()
    val x: i32 = v.mean()
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::TensorReduceMeanNotFloat { .. })),
        "an integer mean should be rejected; got {errors:?}"
    );
}

#[test]
fn a_reduction_over_no_elements_is_rejected() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val v: Tensor<i32, [0]> = []
    val x: i32 = v.max()
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::TensorReduceEmpty { .. })),
        "an empty reduction should be rejected; got {errors:?}"
    );
}

#[test]
fn an_axis_outside_the_rank_is_rejected() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val m: Tensor<i32, [2, 2]> = Tensor::<i32, [2, 2]>::ones()
    val x: Tensor<i32, [2]> = m.sum(4)
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::TensorAxisOutOfRange { .. })),
        "an out-of-range axis should be rejected; got {errors:?}"
    );
}

#[test]
fn a_shape_parameter_leaves_the_run_length_unknown() {
    let errors = semantic_errors(
        r#"
func total<N>(t: Tensor<i32, [N]>) -> i32 {
    t.sum()
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
        "a symbolic extent should be rejected; got {errors:?}"
    );
}

#[test]
fn a_dynamic_axis_leaves_the_run_length_unknown() {
    let errors = semantic_errors(
        r#"
func total(t: Tensor<f32, [?, 4]>) -> f32 {
    t.sum()
}

func main() -> i32 {
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::TensorDynamicExtent { .. })),
        "a dynamic extent should be rejected; got {errors:?}"
    );
}
