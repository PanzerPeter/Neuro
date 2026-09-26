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
fn grad_with_an_argument_other_than_wrt_is_not_supported_yet() {
    for (src, needle) in [
        (
            "@grad(w)\nfunc loss(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> { Tensor::scalar(w.sum()) }\n",
            "w)",
        ),
        (
            "@grad(depth: 2)\nfunc loss(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> { Tensor::scalar(w.sum()) }\n",
            "depth",
        ),
        (
            "@grad(order: 2, order: 2)\nfunc loss(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> { Tensor::scalar(w.sum()) }\n",
            "order: 2)",
        ),
        (
            "@grad(wrt: [w], wrt: [w])\nfunc loss(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> { Tensor::scalar(w.sum()) }\n",
            "wrt: [w])",
        ),
    ] {
        let error = single_error(src);
        assert!(
            matches!(error, TypeError::GradFormUnsupported { .. }),
            "got {error:?}"
        );
        assert_eq!(error.span().start, offset_of(src, needle), "{src}");
    }
}

/// What `wrt:` leaves out is a constant, and a constant may be passed however the
/// function wants.
#[test]
fn wrt_differentiates_only_what_it_lists() {
    let errors = semantic_errors(
        r#"
@grad(wrt: [w])
func loss(w: &mut Tensor<f32, [2]>, x: &Tensor<f32, [2]>, t: Tensor<f32, [2]>, f: &mut Tensor<f32, [2]>, n: Tensor<i32, [2]>) -> Tensor<f32, []> {
    val p = w * x
    Tensor::scalar(p.sum() + t.sum() + f.sum())
}
"#,
    );
    assert!(errors.is_empty(), "got {errors:?}");
}

/// A `wrt:` field path runs through exported fields and literal array positions.
#[test]
fn wrt_field_paths_through_the_receiver_are_accepted() {
    let errors = semantic_errors(
        r#"
struct Layer { export w: Tensor<f32, [2]> }
struct Net { export layer: Layer, export heads: [Tensor<f32, [2]>; 2], hidden: f32 }
impl Net {
    @grad(wrt: [self.layer.w, self.heads[1], k])
    func loss(&mut self, k: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
        Tensor::scalar(self.layer.w.sum() + self.heads[1].sum() + k.sum() * self.hidden)
    }
}
"#,
    );
    assert!(errors.is_empty(), "got {errors:?}");
}

/// Each `wrt:` entry that selects nothing is reported at the entry.
#[test]
fn a_wrt_entry_that_selects_nothing_is_rejected_where_it_is_written() {
    let net =
        "struct Inner { export w: Tensor<f32, [2]>, hidden: Tensor<f32, [2]>, export n: f32 }\n\
               struct Net { export inner: Inner, export heads: [Tensor<f32, [2]>; 2] }\n";
    for entry in [
        "self.inner.hidden",
        "self.inner.n",
        "self.heads[2]",
        "self.nope",
        "self",
        "q",
        "s",
        "x",
    ] {
        let src = format!(
            "{net}impl Net {{\n    @grad(wrt: [{entry}])\n    func loss(&mut self, x: &Tensor<f32, [2]>, s: f32) -> Tensor<f32, []> {{ Tensor::scalar(x.sum() * s) }}\n}}\n"
        );
        let error = single_error(&src);
        assert!(
            matches!(error, TypeError::GradSignature { .. }),
            "{entry}: got {error:?}"
        );
        // A listed parameter that cannot be differentiated is reported at its declaration.
        let at = match entry {
            "x" => offset_of(&src, "x: &"),
            _ => offset_of(&src, &format!("[{entry}]")) + 1,
        };
        assert_eq!(error.span().start, at, "{entry}");
    }
}

#[test]
fn a_field_path_needs_a_mut_receiver_and_a_method() {
    let src = r#"
struct Net { export w: Tensor<f32, [2]> }
impl Net {
    @grad(wrt: [self.w])
    func loss(&self) -> Tensor<f32, []> { Tensor::scalar(self.w.sum()) }
}
"#;
    let error = single_error(src);
    assert!(
        matches!(error, TypeError::GradSignature { .. }),
        "got {error:?}"
    );
    assert_eq!(error.span().start, offset_of(src, "self.w]"));

    let src = r#"
@grad(wrt: [self.w, w])
func loss(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> { Tensor::scalar(w.sum()) }
"#;
    let error = single_error(src);
    assert!(
        matches!(error, TypeError::GradSignature { .. }),
        "got {error:?}"
    );
    assert_eq!(error.span().start, offset_of(src, "self.w"));
}

#[test]
fn a_wrt_that_is_empty_repeated_or_not_a_list_is_rejected() {
    for (wrt, at) in [("[]", "[]"), ("[w, w]", "w]"), ("w", "w)")] {
        let src = format!(
            "@grad(wrt: {wrt})\nfunc loss(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {{ Tensor::scalar(w.sum()) }}\n"
        );
        let error = single_error(&src);
        assert!(
            matches!(error, TypeError::GradSignature { .. }),
            "{wrt}: got {error:?}"
        );
        assert_eq!(error.span().start, offset_of(&src, at), "{wrt}");
    }
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

/// `order: 1` is the default spelled out; `order: 2` also fills `.hessian()`, alone or
/// beside `wrt:`, and on a generic function.
#[test]
fn order_one_and_two_are_accepted_on_a_function() {
    let errors = semantic_errors(
        r#"
@grad(order: 1)
func first(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    Tensor::scalar(w.sum())
}

@grad(order: 2)
func second(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    val s = w * w
    Tensor::scalar(s.sum())
}

@grad(wrt: [w], order: 2)
func selected(w: &mut Tensor<f32, [2]>, x: &Tensor<f32, [2]>) -> Tensor<f32, []> {
    val s = w * x
    Tensor::scalar(s.sum())
}

@grad(order: 2)
func generic<N>(w: &mut Tensor<f32, [N]>) -> Tensor<f32, []> {
    val first = w[0]
    Tensor::scalar(first * first)
}
"#,
    );
    assert!(errors.is_empty(), "got {errors:?}");
}

/// Only the first and second derivatives have an accessor, so any other order, or one the
/// compiler cannot read off the attribute, is refused at the value.
#[test]
fn an_order_with_no_accessor_is_rejected_at_the_value() {
    for (order, needle) in [("0", "0)"), ("3", "3)"), ("n", "n)"), ("2.0", "2.0)")] {
        let src = format!(
            "@grad(order: {order})\nfunc loss(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {{ Tensor::scalar(w.sum()) }}\n"
        );
        let error = single_error(&src);
        assert!(
            matches!(error, TypeError::GradOrderUnsupported { .. }),
            "got {error:?}"
        );
        assert_eq!(error.span().start, offset_of(&src, needle), "{src}");
    }
}

/// A method's derivative takes first derivatives only; `order: 2` there is refused at the
/// argument rather than silently ignored.
#[test]
fn order_two_on_a_method_is_not_supported_yet() {
    let src = r#"
struct Net { scale: f32 }
impl Net {
    @grad(order: 2)
    func loss(&self, w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
        Tensor::scalar(w.sum() * self.scale)
    }
}
"#;
    let error = single_error(src);
    assert!(
        matches!(error, TypeError::GradFormUnsupported { .. }),
        "got {error:?}"
    );
    assert_eq!(error.span().start, offset_of(src, "order"));
}
