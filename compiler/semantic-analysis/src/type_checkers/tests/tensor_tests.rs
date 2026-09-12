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
/// rather than an `f64` one being narrowed, the same rule `val x: f32 = 0.01` follows.
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

/// The `Device` enum these ownership tests need. The unit-test harness type-checks bare
/// source without the prelude, so the program that would get it implicitly has to declare
/// it, which is also what a `@no_prelude` module does.
const DEVICE_DECL: &str = r#"
enum Device {
    CPU,
    GPU(i32)
}
"#;

fn errors_with_device(body: &str) -> Vec<TypeError> {
    semantic_errors(&format!("{DEVICE_DECL}{body}"))
}

#[test]
fn clone_does_not_move_the_tensor() {
    let errors = errors_with_device(
        r#"
func take(t: Tensor<f32, [2, 2]>) -> i32 { return 1 }

func main() -> i32 {
    val a = Tensor::<f32, [2, 2]>::identity()
    val b = a.clone()
    return take(a) + take(b)
}
"#,
    );
    assert!(errors.is_empty(), "clone must not move; got {errors:?}");
}

/// Cloning through a borrow is the copy path for a tensor someone else owns, so it has to
/// hand back an owned `Tensor<T, S>` rather than the borrow it was called on.
#[test]
fn clone_through_a_borrow_yields_an_owned_tensor() {
    let errors = errors_with_device(
        r#"
func take(t: Tensor<f32, [2, 2]>) -> i32 { return 1 }

func copy_of(t: &Tensor<f32, [2, 2]>) -> i32 {
    return take(t.clone())
}

func main() -> i32 {
    return 0
}
"#,
    );
    assert!(errors.is_empty(), "borrowed clone; got {errors:?}");
}

#[test]
fn a_device_transfer_moves_the_receiver() {
    let errors = errors_with_device(
        r#"
func take(t: Tensor<f32, [2, 2]>) -> i32 { return 1 }

func main() -> i32 {
    val a = Tensor::<f32, [2, 2]>::identity()
    val moved = a.to(Device::CPU)
    return take(a)
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::UseOfMovedValue { .. })),
        "`.to` consumes the tensor; got {errors:?}"
    );
}

/// A borrow cannot be consumed, so `.to` is not offered on one; the alternative would be
/// moving a tensor out from under whoever owns it.
#[test]
fn a_device_transfer_is_rejected_on_a_borrow() {
    let errors = errors_with_device(
        r#"
func send(t: &Tensor<f32, [2, 2]>) -> i32 {
    val moved = t.to(Device::CPU)
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
        "`.to` needs a value receiver; got {errors:?}"
    );
}

#[test]
fn a_device_transfer_requires_a_device_argument() {
    let errors = errors_with_device(
        r#"
func main() -> i32 {
    val a = Tensor::<f32, [2, 2]>::identity()
    val moved = a.to(7)
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::Mismatch { .. })),
        "`.to` takes a Device; got {errors:?}"
    );
}

#[test]
fn a_compound_assignment_accepts_an_owned_and_a_borrowed_operand() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    mut w: Tensor<f32, [2, 2]> = [[1.0, 2.0], [3.0, 4.0]]
    val g: Tensor<f32, [2, 2]> = [[0.5, 0.5], [0.5, 0.5]]
    w -= &g
    w += Tensor::<f32, [2, 2]>::ones()
    w *= &g
    w /= &g
    w %= &g
    return 0
}
"#,
    );
    assert!(
        errors.is_empty(),
        "every arithmetic compound assignment applies to a tensor; got {errors:?}"
    );
}

#[test]
fn a_borrowed_operand_survives_the_compound_assignment() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    mut w: Tensor<f32, [2]> = [1.0, 2.0]
    val g: Tensor<f32, [2]> = [0.5, 0.5]
    w += &g
    w += &g
    return 0
}
"#,
    );
    assert!(
        errors.is_empty(),
        "a borrowed operand is read, not consumed; got {errors:?}"
    );
}

#[test]
fn an_owned_operand_is_consumed_by_the_compound_assignment() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    mut w: Tensor<f32, [2]> = [1.0, 2.0]
    val g: Tensor<f32, [2]> = [0.5, 0.5]
    w += g
    w += g
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::UseOfMovedValue { .. })),
        "an owned operand moves into the update; got {errors:?}"
    );
}

#[test]
fn a_compound_assignment_rejects_a_differently_shaped_operand() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    mut w: Tensor<f32, [2, 2]> = [[1.0, 2.0], [3.0, 4.0]]
    val g: Tensor<f32, [3, 3]> = [
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0]
    ]
    w += &g
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::Mismatch { .. })),
        "the shape is part of the type; got {errors:?}"
    );
}

#[test]
fn a_compound_assignment_requires_a_mut_target() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val w: Tensor<f32, [2]> = [1.0, 2.0]
    val g: Tensor<f32, [2]> = [0.5, 0.5]
    w += &g
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::AssignToImmutable { .. })),
        "an in-place update needs an exclusive borrow; got {errors:?}"
    );
}

#[test]
fn a_compound_assignment_rejects_an_element_type_without_arithmetic() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    mut w: Tensor<f16, [2]> = [1.0, 2.0]
    val g: Tensor<f16, [2]> = [0.5, 0.5]
    w += &g
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::TensorElementNotArithmetic { .. })),
        "half precision stops short of arithmetic; got {errors:?}"
    );
}

#[test]
fn a_compound_assignment_cannot_take_its_own_target_as_the_operand() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    mut w: Tensor<f32, [2]> = [1.0, 2.0]
    w += w
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::UseOfMovedValue { .. })),
        "the operand moved the target it updates; got {errors:?}"
    );
}

/// An axis given a position is dropped and an axis given a range survives, so
/// naming every axis with a position is what reads one element.
#[test]
fn indexing_every_axis_reads_an_element_and_a_range_keeps_its_axis() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val m: Tensor<i32, [3, 4]> = [
        [0, 1, 2, 3],
        [4, 5, 6, 7],
        [8, 9, 10, 11]
    ]
    val element: i32 = m[1, 2]
    val row: Tensor<i32, [4]> = m[0, ..]
    val column: Tensor<i32, [3]> = m[.., 1]
    val block: Tensor<i32, [2, 2]> = m[1..3, 2..4]
    val inclusive: Tensor<i32, [2, 3]> = m[0..=1, 0..=2]
    return element
}
"#,
    );
    assert!(
        errors.is_empty(),
        "every index form should check: {errors:?}"
    );
}

#[test]
fn an_index_must_name_every_axis() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val m: Tensor<i32, [2, 3]> = [[1, 2, 3], [4, 5, 6]]
    val bad = m[0]
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::TensorIndexRankMismatch { .. })),
        "a rank-2 tensor takes two arguments; got {errors:?}"
    );
}

#[test]
fn a_constant_position_outside_its_axis_is_rejected() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val m: Tensor<i32, [2, 3]> = [[1, 2, 3], [4, 5, 6]]
    val bad = m[2, 0]
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::TensorIndexOutOfBounds { .. })),
        "axis 0 has extent 2; got {errors:?}"
    );
}

#[test]
fn a_slice_past_the_extent_is_rejected() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val m: Tensor<i32, [2, 3]> = [[1, 2, 3], [4, 5, 6]]
    val bad = m[0..1, 1..9]
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::TensorSliceOutOfRange { .. })),
        "axis 1 has extent 3; got {errors:?}"
    );
}

/// A slice's extents are part of the result's type, so a bound that is only known at
/// run time has no type to produce — unlike a position, which may be any integer.
#[test]
fn a_runtime_slice_bound_is_rejected_while_a_runtime_position_is_not() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val m: Tensor<i32, [2, 3]> = [[1, 2, 3], [4, 5, 6]]
    mut k = 1
    val fine = m[k, k]
    val bad = m[0..1, 0..k]
    return fine
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::TensorSliceBoundNotConstant { .. })),
        "a run-time slice bound has no static extent; got {errors:?}"
    );
    assert_eq!(
        errors.len(),
        1,
        "the run-time position is legal; got {errors:?}"
    );
}

/// Indexing reads through a borrow without consuming the tensor, which is what lets a
/// borrowed weight be inspected inside a loop.
#[test]
fn a_borrowed_tensor_is_indexed_and_not_moved() {
    let errors = semantic_errors(
        r#"
func first(t: &Tensor<i32, [2, 2]>) -> i32 {
    return t[0, 0]
}

func main() -> i32 {
    val m: Tensor<i32, [2, 2]> = [[1, 2], [3, 4]]
    val a = first(&m)
    val b = first(&m)
    return a + b
}
"#,
    );
    assert!(errors.is_empty(), "indexing reads a tensor: {errors:?}");
}

/// A range index is a tensor form: an array takes one integer and offers `.slice`.
#[test]
fn a_range_index_over_an_array_is_rejected() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val xs = [1, 2, 3]
    val bad = xs[0..2]
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::TensorIndexOnNonTensor { .. })),
        "an array is not sliced by an index; got {errors:?}"
    );
}

#[test]
fn a_shape_parameter_is_inferred_from_the_argument() {
    let errors = semantic_errors(
        r#"
func first<M, K>(t: &Tensor<i32, [M, K]>) -> i32 {
    return t[0, 0]
}

func main() -> i32 {
    val wide: Tensor<i32, [2, 3]> = [[1, 2, 3], [4, 5, 6]]
    val tall: Tensor<i32, [3, 2]> = [[1, 2], [3, 4], [5, 6]]
    return first(&wide) + first(&tall)
}
"#,
    );
    assert!(errors.is_empty(), "shape generics; got {errors:?}");
}

#[test]
fn a_repeated_shape_parameter_must_agree() {
    let errors = semantic_errors(
        r#"
func pair<M, N, K>(a: &Tensor<i32, [M, K]>, b: &Tensor<i32, [K, N]>) -> i32 {
    return a[0, 0] + b[0, 0]
}

func main() -> i32 {
    val x: Tensor<i32, [2, 3]> = [[1, 2, 3], [4, 5, 6]]
    val y: Tensor<i32, [5, 6]> = Tensor::<i32, [5, 6]>::zeros()
    return pair(&x, &y)
}
"#,
    );
    assert!(
        errors.iter().any(|e| matches!(
            e,
            TypeError::TensorShapeParamConflict { name, expected, found, .. }
                if name == "K" && *expected == 3 && *found == 5
        )),
        "expected a conflict naming K; got {errors:?}"
    );
}

#[test]
fn a_shape_parameters_extent_reaches_the_return_type() {
    let errors = semantic_errors(
        r#"
func row<M, K>(t: &Tensor<i32, [M, K]>) -> Tensor<i32, [K]> {
    return t[0, ..]
}

func main() -> i32 {
    val grid: Tensor<i32, [2, 3]> = [[1, 2, 3], [4, 5, 6]]
    val first: Tensor<i32, [3]> = row(&grid)
    return first[2]
}
"#,
    );
    assert!(errors.is_empty(), "sliced shape parameter; got {errors:?}");
}

#[test]
fn a_value_predicate_over_a_shape_parameter_is_enforced() {
    let errors = semantic_errors(
        r#"
func wide<N>(t: &Tensor<i32, [N]>) -> i32 where N > 2 {
    return t[0]
}

func main() -> i32 {
    val pair: Tensor<i32, [2]> = [1, 2]
    return wide(&pair)
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::ConstPredicateViolated { .. })),
        "expected the predicate to be violated; got {errors:?}"
    );
}

#[test]
fn an_undeclared_tensor_dimension_is_named() {
    let errors = semantic_errors(
        r#"
func read(t: &Tensor<i32, [Q]>) -> i32 {
    return t[0]
}

func main() -> i32 {
    return 0
}
"#,
    );
    assert!(
        errors.iter().any(|e| matches!(
            e,
            TypeError::UnknownTensorDimension { name, .. } if name == "Q"
        )),
        "expected the dimension to be named; got {errors:?}"
    );
}

/// A literal is written once and every instantiation reuses it, so there is no length
/// the extent could be checked against.
#[test]
fn a_literal_against_a_symbolic_extent_is_rejected() {
    let errors = semantic_errors(
        r#"
func build<N>() -> Tensor<i32, [N]> {
    val t: Tensor<i32, [N]> = [1, 2, 3]
    return t
}

func main() -> i32 {
    return 0
}
"#,
    );
    assert!(
        errors.iter().any(|e| matches!(
            e,
            TypeError::TensorLiteralSymbolicExtent { name, .. } if name == "N"
        )),
        "expected the symbolic extent to be reported; got {errors:?}"
    );
}

#[test]
fn a_named_shape_type_checks_and_matches_an_unnamed_one() {
    let errors = semantic_errors(
        r#"
func rows(image: Tensor<f32, [channels: 3, height: 4, width: 4]>) -> i32 {
    return 3
}

func main() -> i32 {
    val plain: Tensor<f32, [3, 4, 4]> = Tensor::<f32, [3, 4, 4]>::zeros()
    val named: Tensor<f32, [channels: 3, height: 4, width: 4]> = plain
    return rows(named)
}
"#,
    );
    assert!(
        errors.is_empty(),
        "a named shape is interchangeable with the unnamed one; got {errors:?}"
    );
}

#[test]
fn a_transposed_argument_is_rejected_by_its_axis_names() {
    let errors = semantic_errors(
        r#"
func normalize(x: Tensor<f32, [height: 4, width: 4]>) { }

func main() -> i32 {
    val t: Tensor<f32, [width: 4, height: 4]> = Tensor::<f32, [4, 4]>::zeros()
    normalize(t)
    return 0
}
"#,
    );
    assert!(
        errors.iter().any(|e| matches!(
            e,
            TypeError::TensorAxisNameMismatch { axis, expected, found, .. }
                if *axis == 0 && expected == "height" && found == "width"
        )),
        "a transposed tensor whose extents agree; got {errors:?}"
    );
}

#[test]
fn a_transposed_argument_is_rejected_along_shape_parameters() {
    let errors = semantic_errors(
        r#"
func project<H, W>(x: Tensor<f32, [height: H, width: W]>) { }

func main() -> i32 {
    val t: Tensor<f32, [width: 4, height: 4]> = Tensor::<f32, [4, 4]>::zeros()
    project(t)
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::TensorAxisNameMismatch { .. })),
        "symbolic extents bind but the names still disagree; got {errors:?}"
    );
}

#[test]
fn a_transposed_annotation_is_rejected_at_the_binding() {
    let errors = semantic_errors(
        r#"
func make() -> Tensor<f32, [height: 4, width: 4]> {
    return Tensor::<f32, [4, 4]>::zeros()
}

func main() -> i32 {
    val t: Tensor<f32, [width: 4, height: 4]> = make()
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::TensorAxisNameMismatch { .. })),
        "the annotation transposes the initializer's axes; got {errors:?}"
    );
}

#[test]
fn a_repeated_dimension_name_is_rejected() {
    let errors = semantic_errors(
        r#"
func square(x: Tensor<f32, [side: 4, side: 4]>) { }

func main() -> i32 {
    return 0
}
"#,
    );
    assert!(
        errors.iter().any(|e| matches!(
            e,
            TypeError::DuplicateTensorAxisName { name, .. } if name == "side"
        )),
        "two axes cannot share one name; got {errors:?}"
    );
}

#[test]
fn a_dimension_name_does_not_declare_a_shape_parameter() {
    let errors = semantic_errors(
        r#"
func widths<W>(x: Tensor<f32, [batch: 2, width: W]>) -> i32 {
    return 2
}

func main() -> i32 {
    val t: Tensor<f32, [batch: 2, width: 3]> = Tensor::<f32, [2, 3]>::zeros()
    return widths(t)
}
"#,
    );
    assert!(
        errors.is_empty(),
        "`W` is the shape parameter, `width` is only the axis name; got {errors:?}"
    );
}

#[test]
fn a_sliced_axis_keeps_its_dimension_name() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val image: Tensor<f32, [height: 2, width: 3]> = [
        [1.0, 2.0, 3.0],
        [4.0, 5.0, 6.0]
    ]
    val row: Tensor<f32, [width: 3]> = image[0, ..]
    return 0
}
"#,
    );
    assert!(
        errors.is_empty(),
        "the surviving axis keeps the name it had; got {errors:?}"
    );
}

#[test]
fn a_sliced_axis_does_not_answer_to_another_name() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val image: Tensor<f32, [height: 2, width: 3]> = [
        [1.0, 2.0, 3.0],
        [4.0, 5.0, 6.0]
    ]
    val row: Tensor<f32, [height: 3]> = image[0, ..]
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::TensorAxisNameMismatch { .. })),
        "the axis that survived is `width`; got {errors:?}"
    );
}

#[test]
fn an_identity_matrix_may_name_its_two_axes_differently() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val eye: Tensor<f32, [row: 4, col: 4]> = Tensor::<f32, [row: 4, col: 4]>::identity()
    return 0
}
"#,
    );
    assert!(
        errors.is_empty(),
        "squareness is a property of the extents; got {errors:?}"
    );
}

#[test]
fn a_dynamic_axis_accepts_any_extent_at_that_position() {
    let errors = semantic_errors(
        r#"
func rows(batch: &Tensor<f32, [?, 4]>) -> i32 {
    return 4
}

func main() -> i32 {
    val small: Tensor<f32, [2, 4]> = [[1.0, 2.0, 3.0, 4.0], [5.0, 6.0, 7.0, 8.0]]
    val big = Tensor::<f32, [7, 4]>::zeros()
    return rows(&small) + rows(&big)
}
"#,
    );
    assert!(
        errors.is_empty(),
        "two extents at one `?` axis; got {errors:?}"
    );
}

#[test]
fn a_static_axis_beside_a_dynamic_one_is_still_checked() {
    let errors = semantic_errors(
        r#"
func rows(batch: &Tensor<f32, [?, 4]>) -> i32 {
    return 4
}

func main() -> i32 {
    val wrong = Tensor::<f32, [2, 8]>::zeros()
    return rows(&wrong)
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::Mismatch { .. })),
        "the second axis is 4, not 8; got {errors:?}"
    );
}

#[test]
fn a_dynamic_shape_does_not_satisfy_a_static_annotation() {
    let errors = semantic_errors(
        r#"
func widen(t: Tensor<f32, [2, 4]>) -> Tensor<f32, [?, 4]> {
    return t
}

func main() -> i32 {
    val narrowed: Tensor<f32, [2, 4]> = widen(Tensor::<f32, [2, 4]>::zeros())
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::Mismatch { .. })),
        "widening is sound, narrowing is not; got {errors:?}"
    );
}

#[test]
fn a_dynamic_extent_cannot_be_constructed_or_written_as_a_literal() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val zeroed = Tensor::<f32, [?, 4]>::zeros()
    val written: Tensor<f32, [?, 2]> = [[1.0, 2.0]]
    return 0
}
"#,
    );
    let reported = errors
        .iter()
        .filter(|e| matches!(e, TypeError::TensorDynamicExtent { .. }))
        .count();
    assert_eq!(
        reported, 2,
        "neither a constructor nor a literal has a size; got {errors:?}"
    );
}

#[test]
fn a_dynamic_extent_is_refused_by_every_operation_that_needs_its_value() {
    let errors = semantic_errors(
        r#"
func flexible(x: Tensor<f32, [?, 4]>) -> i32 {
    val copied = x.clone()
    val element = x[0, 1]
    val transposed = x.t()
    return 0
}

func main() -> i32 {
    return 0
}
"#,
    );
    let reported = errors
        .iter()
        .filter(|e| matches!(e, TypeError::TensorDynamicExtent { .. }))
        .count();
    assert_eq!(
        reported, 3,
        "a copy, an index and a shape cast each need an extent; got {errors:?}"
    );
}

#[test]
fn a_dynamic_axis_binds_no_shape_parameter() {
    let errors = semantic_errors(
        r#"
func widen(t: Tensor<f32, [2, 4]>) -> Tensor<f32, [?, 4]> {
    return t
}

func rows<N>(t: &Tensor<f32, [N, 4]>) -> i32 {
    return 4
}

func main() -> i32 {
    val dynamic = widen(Tensor::<f32, [2, 4]>::zeros())
    return rows(&dynamic)
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::Mismatch { .. })),
        "`N` has no value to take from a `?` axis; got {errors:?}"
    );
}
