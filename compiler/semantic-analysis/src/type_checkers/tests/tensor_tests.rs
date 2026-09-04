use super::semantic_errors;
use crate::errors::TypeError;

#[test]
fn static_tensor_annotations_type_check() {
    let errors = semantic_errors(
        r#"
type Weights = Tensor<f32, [784, 128]>

struct Layer {
    bias: Tensor<f32, [128]>
}

func forward(w: Weights, x: Tensor<f32, [128]>, loss: Tensor<f32, []>) { }

func main() -> i32 {
    return 0
}
"#,
    );
    assert!(
        errors.is_empty(),
        "valid tensor annotations; got {errors:?}"
    );
}

#[test]
fn a_borrowed_tensor_is_accepted_and_shares_the_shape() {
    let errors = semantic_errors(
        r#"
func read(w: &Tensor<f32, [2, 2]>) { }

func caller(w: &Tensor<f32, [2, 2]>) {
    read(w)
    read(w)
}

func main() -> i32 {
    return 0
}
"#,
    );
    assert!(errors.is_empty(), "borrowed tensors; got {errors:?}");
}

#[test]
fn a_shape_mismatch_is_a_type_error() {
    let errors = semantic_errors(
        r#"
func takes_square(t: Tensor<f32, [3, 3]>) { }

func pass_through(t: Tensor<f32, [2, 2]>) {
    takes_square(t)
}

func main() -> i32 {
    return 0
}
"#,
    );
    assert!(
        errors.iter().any(|e| matches!(
            e,
            TypeError::Mismatch { expected, found, .. }
                if expected.to_string() == "Tensor<f32, [3, 3]>"
                    && found.to_string() == "Tensor<f32, [2, 2]>"
        )),
        "expected a shape mismatch; got {errors:?}"
    );
}

#[test]
fn an_element_type_mismatch_is_a_type_error() {
    let errors = semantic_errors(
        r#"
func takes_f32(t: Tensor<f32, [2, 2]>) { }

func pass_through(t: Tensor<f64, [2, 2]>) {
    takes_f32(t)
}

func main() -> i32 {
    return 0
}
"#,
    );
    assert!(
        errors.iter().any(|e| matches!(
            e,
            TypeError::Mismatch { expected, found, .. }
                if expected.to_string() == "Tensor<f32, [2, 2]>"
                    && found.to_string() == "Tensor<f64, [2, 2]>"
        )),
        "expected an element mismatch; got {errors:?}"
    );
}

#[test]
fn a_tensor_is_not_copy_so_passing_it_twice_moves_it() {
    let errors = semantic_errors(
        r#"
func consume(t: Tensor<f32, [2, 2]>) { }

func twice(t: Tensor<f32, [2, 2]>) {
    consume(t)
    consume(t)
}

func main() -> i32 {
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::UseOfMovedValue { .. })),
        "expected a use-after-move; got {errors:?}"
    );
}

#[test]
fn a_non_scalar_tensor_element_is_rejected() {
    let errors = semantic_errors(
        r#"
func bad(t: Tensor<string, [2]>) { }

func main() -> i32 {
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::NonScalarTensorElement { .. })),
        "expected a non-scalar element error; got {errors:?}"
    );
}

/// A shape-less `Tensor<f32>` names a real type written wrong, so the diagnostic points
/// at the missing shape rather than reporting the name as not generic.
#[test]
fn a_tensor_without_a_shape_asks_for_one() {
    let errors = semantic_errors(
        r#"
func bad(t: Tensor<f32>) { }

func main() -> i32 {
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::TensorShapeRequired { .. })),
        "expected a missing-shape error; got {errors:?}"
    );
}

/// `Tensor` is a prelude name, not a keyword: a module declaring its own generic
/// `Tensor` keeps it.
#[test]
fn a_locally_declared_tensor_type_shadows_the_prelude_name() {
    let errors = semantic_errors(
        r#"
struct Tensor<T> {
    value: T
}

func main() -> i32 {
    val t: Tensor<i32> = Tensor { value: 1 }
    return t.value
}
"#,
    );
    assert!(errors.is_empty(), "shadowed tensor name; got {errors:?}");
}
