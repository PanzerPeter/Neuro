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

#[test]
fn a_nested_array_literal_coerces_under_a_tensor_annotation() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val v: Tensor<f32, [3]> = [1.0, 2.0, 3.0]
    val m: Tensor<f32, [2, 3]> = [
        [1.0, 2.0, 3.0],
        [4.0, 5.0, 6.0]
    ]
    return 0
}
"#,
    );
    assert!(errors.is_empty(), "tensor literals; got {errors:?}");
}

/// The annotation types each leaf, so an `f32` tensor's `1.0` is an `f32` literal
/// rather than an `f64` one being narrowed — the same rule `val x: f32 = 0.01` follows.
/// A half-precision element still needs its suffix, exactly as a half-precision scalar
/// binding does: the tensor path does not widen literal inference.
#[test]
fn a_tensor_literal_element_is_typed_by_the_annotation() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val v: Tensor<f32, [2]> = [1.0, 2.0]
    val w: Tensor<i64, [2]> = [1, 2]
    val h: Tensor<f16, [2]> = [1.0f16, 2.0f16]
    return 0
}
"#,
    );
    assert!(errors.is_empty(), "annotation-typed leaves; got {errors:?}");
}

/// A value that already has a type is not converted for the annotation's benefit.
#[test]
fn a_typed_element_of_the_wrong_type_is_rejected() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val d: f64 = 1.0
    val v: Tensor<f32, [2]> = [d, 2.0]
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::Mismatch { .. })),
        "expected a mismatch; got {errors:?}"
    );
}

#[test]
fn a_ragged_tensor_literal_is_rejected() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val m: Tensor<f32, [2, 3]> = [[1.0, 2.0, 3.0], [4.0, 5.0]]
    return 0
}
"#,
    );
    assert!(
        errors.iter().any(|e| matches!(
            e,
            TypeError::TensorExtentMismatch {
                expected: 3,
                found: 2,
                ..
            }
        )),
        "expected an extent mismatch; got {errors:?}"
    );
}

#[test]
fn a_literal_shallower_than_the_shape_is_a_rank_error() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val m: Tensor<f32, [2, 2]> = [1.0, 2.0]
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::TensorRankMismatch { .. })),
        "expected a rank mismatch; got {errors:?}"
    );
}

#[test]
fn a_rank_zero_tensor_has_no_literal_form() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val s: Tensor<f32, []> = [1.0]
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::TensorScalarNeedsConstructor { .. })),
        "expected the scalar-constructor hint; got {errors:?}"
    );
}

/// Without an annotation the literal is a plain array, which is what keeps
/// `[1.0, 2.0, 3.0]` an `[f64; 3]`.
#[test]
fn an_unannotated_array_literal_is_not_a_tensor() {
    let errors = semantic_errors(
        r#"
func takes_array(a: [f64; 3]) { }

func main() -> i32 {
    val a = [1.0, 2.0, 3.0]
    takes_array(a)
    return 0
}
"#,
    );
    assert!(errors.is_empty(), "plain array; got {errors:?}");
}

#[test]
fn every_construction_helper_type_checks() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val z = Tensor::<f32, [3, 3]>::zeros()
    val o = Tensor::<f32, [3, 3]>::ones()
    val e = Tensor::<f32, [4, 4]>::identity()
    val r = Tensor::<f32, [8, 4]>::random_normal(0.0f32, 0.02f32)
    val s: Tensor<f32, []> = Tensor::scalar(42.0)
    val v = Tensor::<f32, [3]>::from([1.0, 2.0, 3.0])
    return 0
}
"#,
    );
    assert!(errors.is_empty(), "construction helpers; got {errors:?}");
}

#[test]
fn a_constructor_with_no_type_to_build_is_reported() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val z = Tensor::zeros()
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::TensorTypeNotInferable { .. })),
        "expected the inference hint; got {errors:?}"
    );
}

#[test]
fn an_unknown_constructor_lists_the_ones_that_exist() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val z = Tensor::<f32, [2, 2]>::eye()
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::UnknownTensorConstructor { .. })),
        "expected an unknown-constructor error; got {errors:?}"
    );
}

#[test]
fn identity_requires_a_square_rank_two_shape() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val e = Tensor::<f32, [2, 3]>::identity()
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::TensorConstructorNotApplicable { .. })),
        "expected an inapplicable-constructor error; got {errors:?}"
    );
}

#[test]
fn random_normal_draws_only_into_full_precision_floats() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val r = Tensor::<i32, [2, 2]>::random_normal(0, 1)
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::TensorConstructorNotApplicable { .. })),
        "expected an inapplicable-constructor error; got {errors:?}"
    );
}

/// The prelude's shadowing promise: a program that declares its own `Tensor` keeps the name,
/// so the builtin constructors stand aside for it.
#[test]
fn a_declared_tensor_type_shadows_the_builtin_constructors() {
    let errors = semantic_errors(
        r#"
struct Tensor {
    v: i32
}

impl Tensor {
    func make() -> Tensor {
        return Tensor { v: 7 }
    }
}

func main() -> i32 {
    val t = Tensor::make()
    return t.v
}
"#,
    );
    assert!(errors.is_empty(), "shadowed Tensor; got {errors:?}");
}
