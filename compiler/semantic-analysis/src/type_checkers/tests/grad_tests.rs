// `@grad` signature rules.

use super::semantic_errors;
use crate::errors::TypeError;

/// The byte offset of the first `needle` in `src`, which is where a diagnostic about it
/// must point.
fn offset_of(src: &str, needle: &str) -> usize {
    src.find(needle)
        .unwrap_or_else(|| panic!("`{needle}` not in source"))
}

fn single_error(src: &str) -> TypeError {
    let mut errors = semantic_errors(src);
    assert_eq!(
        errors.len(),
        1,
        "expected exactly one error, got {errors:?}"
    );
    errors.remove(0)
}

#[test]
fn a_function_over_mutably_borrowed_float_tensors_is_accepted() {
    let errors = semantic_errors(
        r#"
@grad
func loss(w: &mut Tensor<f32, [2]>, scale: f32) -> Tensor<f32, []> {
    val sq = w * w
    Tensor::scalar(sq.sum() * scale)
}
"#,
    );
    assert!(errors.is_empty(), "got {errors:?}");
}

#[test]
fn a_loss_that_is_not_a_rank_zero_f32_tensor_is_rejected_at_the_return_type() {
    let src = r#"
@grad
func loss(w: &mut Tensor<f32, [2]>) -> f32 {
    w.sum()
}
"#;
    let error = single_error(src);
    assert!(
        matches!(error, TypeError::GradSignature { .. }),
        "got {error:?}"
    );
    assert_eq!(error.span().start, offset_of(src, "f32 {"));
}

#[test]
fn a_shared_tensor_borrow_is_rejected_at_the_parameter() {
    let src = r#"
@grad
func loss(w: &Tensor<f32, [2]>) -> Tensor<f32, []> {
    Tensor::scalar(w.sum())
}
"#;
    let error = single_error(src);
    assert!(
        matches!(error, TypeError::GradSignature { .. }),
        "got {error:?}"
    );
    assert_eq!(error.span().start, offset_of(src, "w: &"));
}

#[test]
fn an_owned_tensor_parameter_is_rejected() {
    let src = r#"
@grad
func loss(w: Tensor<f32, [2]>) -> Tensor<f32, []> {
    Tensor::scalar(w.sum())
}
"#;
    assert!(matches!(single_error(src), TypeError::GradSignature { .. }));
}

#[test]
fn an_integer_tensor_parameter_is_rejected() {
    let src = r#"
@grad
func loss(w: &mut Tensor<i32, [2]>) -> Tensor<f32, []> {
    Tensor::scalar(1.0)
}
"#;
    assert!(matches!(single_error(src), TypeError::GradSignature { .. }));
}

#[test]
fn a_function_with_nothing_to_differentiate_is_rejected_at_its_name() {
    let src = r#"
@grad
func loss(x: f32) -> Tensor<f32, []> {
    Tensor::scalar(x)
}
"#;
    let error = single_error(src);
    assert!(
        matches!(error, TypeError::GradSignature { .. }),
        "got {error:?}"
    );
    assert_eq!(error.span().start, offset_of(src, "loss("));
}

/// With no `wrt:`, a method differentiates its tensor parameters, and the
/// receiver, a constant, may be borrowed either way.
#[test]
fn a_method_borrowing_its_receiver_is_accepted() {
    let errors = semantic_errors(
        r#"
struct Net { scale: f32 }
impl Net {
    @grad
    func loss(&self, w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
        Tensor::scalar(w.sum() * self.scale)
    }

    @grad
    func tuned(&mut self, w: &mut Tensor<f32, [2]>, rate: f32) -> Tensor<f32, []> {
        Tensor::scalar(w.sum() * rate)
    }
}
"#,
    );
    assert!(errors.is_empty(), "got {errors:?}");
}

/// A method is held to a function's signature rules over the parameters after `self`.
#[test]
fn a_method_is_held_to_the_signature_rules() {
    let src = r#"
struct Net { scale: f32 }
impl Net {
    @grad
    func loss(&self, w: &Tensor<f32, [2]>) -> Tensor<f32, []> {
        Tensor::scalar(w.sum())
    }
}
"#;
    let error = single_error(src);
    assert!(
        matches!(error, TypeError::GradSignature { .. }),
        "got {error:?}"
    );
    assert_eq!(error.span().start, offset_of(src, "w: &Tensor"));
}

#[test]
fn a_method_consuming_its_receiver_is_rejected_at_its_name() {
    let src = r#"
struct Net { scale: f32 }
impl Net {
    @grad
    func loss(self, w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
        Tensor::scalar(w.sum())
    }
}
"#;
    let error = single_error(src);
    assert!(
        matches!(error, TypeError::GradSignature { .. }),
        "got {error:?}"
    );
    assert_eq!(error.span().start, offset_of(src, "loss("));
}

/// The forms a derivative is not yet derived for, each refused at the attribute.
#[test]
fn grad_on_an_associated_function_a_trait_impl_or_a_generic_impl_is_not_supported_yet() {
    for src in [
        r#"
struct Net { scale: f32 }
impl Net {
    @grad
    func loss(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
        Tensor::scalar(w.sum())
    }
}
"#,
        r#"
struct Net { scale: f32 }
trait Objective {
    func loss(&self, w: &mut Tensor<f32, [2]>) -> Tensor<f32, []>
}
impl Objective for Net {
    @grad
    func loss(&self, w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
        Tensor::scalar(w.sum())
    }
}
"#,
        r#"
struct Holder<T> { value: T }
impl<T> Holder<T> {
    @grad
    func loss(&self, w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
        Tensor::scalar(w.sum())
    }
}
"#,
    ] {
        let error = single_error(src);
        assert!(
            matches!(error, TypeError::GradFormUnsupported { .. }),
            "got {error:?}"
        );
        assert_eq!(error.span().start, offset_of(src, "@grad"));
    }
}

#[test]
fn grad_with_arguments_is_not_supported_yet() {
    let src = r#"
@grad(w)
func loss(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    Tensor::scalar(w.sum())
}
"#;
    assert!(matches!(
        single_error(src),
        TypeError::GradFormUnsupported { .. }
    ));
}

/// A generic template is checked with its parameters abstract: a shape parameter is an
/// extent every instance fixes, and the derivative is derived per instance.
#[test]
fn a_shape_generic_function_is_accepted() {
    let errors = semantic_errors(
        r#"
@grad
func loss<N>(w: &mut Tensor<f32, [N]>, scale: f32) -> Tensor<f32, []> {
    val first = w[0]
    Tensor::scalar(first * first * scale)
}
"#,
    );
    assert!(errors.is_empty(), "got {errors:?}");
}

/// Its signature rules still hold: the parameter is `&mut`, as it is without generics.
#[test]
fn a_generic_function_is_held_to_the_signature_rules() {
    let src = r#"
@grad
func loss<N>(w: &Tensor<f32, [N]>) -> Tensor<f32, []> {
    Tensor::scalar(w[0])
}
"#;
    assert!(matches!(single_error(src), TypeError::GradSignature { .. }));
}

#[test]
fn a_declared_struct_named_like_the_bundle_is_a_clash() {
    let src = r#"
struct GradsOf_loss { x: f32 }

@grad
func loss(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    Tensor::scalar(w.sum())
}
"#;
    let error = single_error(src);
    assert!(
        matches!(error, TypeError::GradGeneratedNameTaken { .. }),
        "got {error:?}"
    );
    assert_eq!(error.span().start, offset_of(src, "loss("));
}
