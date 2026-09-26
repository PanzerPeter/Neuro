// `.backward()`, `.grad()` and `.zero_grad()`: the pairing with the `@grad` call, and
// the borrows that run from that call to its `.backward()`.

use super::semantic_errors;
use crate::errors::TypeError;

const LOSS: &str = r#"
@grad
func loss(w: &mut Tensor<f32, [2]>, scale: f32) -> Tensor<f32, []> {
    val s = w * scale
    Tensor::scalar(s.sum())
}
"#;

/// The errors `main`'s body produces beside the `@grad` function above.
fn errors_in_main(body: &str) -> Vec<TypeError> {
    semantic_errors(&format!(
        "{LOSS}\nfunc main() -> i32 {{\n{body}\n    return 0\n}}\n"
    ))
}

fn backward_problem(body: &str) -> String {
    let errors = errors_in_main(body);
    match errors.as_slice() {
        [TypeError::BackwardUnavailable { problem, .. }] => problem.clone(),
        other => panic!("expected one `.backward()` error, got {other:?}"),
    }
}

#[test]
fn the_training_step_of_the_specification_is_accepted() {
    let errors = errors_in_main(
        r#"
    mut w: Tensor<f32, [2]> = [1.0, 2.0]
    pool {
        val l = loss(&mut w, 2.0f32)
        l.backward()
        w -= 0.1f32 * w.grad()
        w.zero_grad()
    }"#,
    );
    assert!(errors.is_empty(), "got {errors:?}");
}

#[test]
fn a_differentiated_argument_is_borrowed_until_the_backward() {
    let errors = errors_in_main(
        r#"
    mut w: Tensor<f32, [2]> = [1.0, 2.0]
    val l = loss(&mut w, 2.0f32)
    val r = &w
    l.backward()"#,
    );
    assert!(
        matches!(
            errors.as_slice(),
            [TypeError::CannotBorrowWhileMutablyBorrowed { .. }]
        ),
        "got {errors:?}"
    );
}

#[test]
fn reading_moving_or_updating_the_argument_before_the_backward_is_refused() {
    for body in [
        "    val m = w\n    l.backward()",
        "    w = [0.0, 0.0]\n    l.backward()",
        "    w -= 1.0f32\n    l.backward()",
    ] {
        let errors = errors_in_main(&format!(
            "    mut w: Tensor<f32, [2]> = [1.0, 2.0]\n    val l = loss(&mut w, 2.0f32)\n{body}"
        ));
        assert_eq!(errors.len(), 1, "{body}: got {errors:?}");
    }
}

#[test]
fn the_backward_ends_the_borrow() {
    let errors = errors_in_main(
        r#"
    mut w: Tensor<f32, [2]> = [1.0, 2.0]
    val l = loss(&mut w, 2.0f32)
    l.backward()
    val r = &w
    val g = w.grad()"#,
    );
    assert!(errors.is_empty(), "got {errors:?}");
}

#[test]
fn a_loss_with_no_backward_borrows_only_for_the_call() {
    // Evaluating a loss twice, as a validation pass does, must not freeze the weights.
    let errors = errors_in_main(
        r#"
    mut w: Tensor<f32, [2]> = [1.0, 2.0]
    val a = loss(&mut w, 2.0f32)
    val b = loss(&mut w, 3.0f32)
    w -= 1.0f32"#,
    );
    assert!(errors.is_empty(), "got {errors:?}");
}

#[test]
fn a_mut_binding_passed_on_is_held_like_a_borrow() {
    let errors = semantic_errors(&format!(
        r#"{LOSS}
func step(r: &mut Tensor<f32, [2]>) {{
    val l = loss(r, 1.0f32)
    val s = r
    l.backward()
}}
"#
    ));
    assert!(
        matches!(
            errors.as_slice(),
            [TypeError::CannotUseWhileMutablyBorrowed { .. }]
        ),
        "got {errors:?}"
    );
}

#[test]
fn backward_on_a_value_no_grad_call_produced_is_refused() {
    let problem = backward_problem(
        r#"
    val t: Tensor<f32, []> = Tensor::scalar(1.0f32)
    t.backward()"#,
    );
    assert!(problem.contains("bound directly"), "{problem}");
}

#[test]
fn backward_on_a_loss_computed_from_the_call_is_refused() {
    let problem = backward_problem(
        r#"
    mut w: Tensor<f32, [2]> = [1.0, 2.0]
    val l = loss(&mut w, 2.0f32)
    val doubled = l * 2.0f32
    doubled.backward()"#,
    );
    assert!(problem.contains("'doubled'"), "{problem}");
}

#[test]
fn backward_on_a_mut_loss_is_refused() {
    let problem = backward_problem(
        r#"
    mut w: Tensor<f32, [2]> = [1.0, 2.0]
    mut l = loss(&mut w, 2.0f32)
    l.backward()"#,
    );
    assert!(problem.contains("`val`"), "{problem}");
}

#[test]
fn backward_on_a_temporary_is_refused() {
    let problem = backward_problem(
        r#"
    mut w: Tensor<f32, [2]> = [1.0, 2.0]
    loss(&mut w, 2.0f32).backward()"#,
    );
    assert!(problem.contains("binding"), "{problem}");
}

#[test]
fn backward_in_another_block_is_refused() {
    let problem = backward_problem(
        r#"
    mut w: Tensor<f32, [2]> = [1.0, 2.0]
    val l = loss(&mut w, 2.0f32)
    if true {
        l.backward()
    }"#,
    );
    assert!(problem.contains("same block"), "{problem}");
}

#[test]
fn backward_runs_once() {
    let problem = backward_problem(
        r#"
    mut w: Tensor<f32, [2]> = [1.0, 2.0]
    val l = loss(&mut w, 2.0f32)
    l.backward()
    l.backward()"#,
    );
    assert!(problem.contains("already ran"), "{problem}");
}

#[test]
fn a_live_gradient_view_blocks_zero_grad() {
    let errors = errors_in_main(
        r#"
    mut w: Tensor<f32, [2]> = [1.0, 2.0]
    val l = loss(&mut w, 2.0f32)
    l.backward()
    val g = w.grad()
    w.zero_grad()
    val first = g[0]"#,
    );
    assert!(
        matches!(
            errors.as_slice(),
            [TypeError::CannotMutablyBorrowWhileBorrowed { .. }]
        ),
        "got {errors:?}"
    );
}

#[test]
fn zero_grad_needs_a_mut_binding() {
    let errors = errors_in_main(
        r#"
    val w: Tensor<f32, [2]> = [1.0, 2.0]
    w.zero_grad()"#,
    );
    assert!(
        matches!(errors.as_slice(), [TypeError::CannotBorrowMutably { .. }]),
        "got {errors:?}"
    );
}

#[test]
fn a_user_method_called_grad_is_not_a_gradient_view() {
    let errors = semantic_errors(
        r#"
struct Layer { scale: f32 }
impl Layer {
    func grad(&self) -> f32 { self.scale }
    func bump(&mut self) { self.scale += 1.0f32 }
}
func main() -> i32 {
    mut layer = Layer { scale: 1.0f32 }
    val g = layer.grad()
    layer.bump()
    return g as i32
}
"#,
    );
    assert!(errors.is_empty(), "got {errors:?}");
}

/// BUG-066: an in-place tensor update skipped the check `=` makes, so it
/// rewrote a buffer a live borrow was reading.
#[test]
fn a_compound_update_of_a_borrowed_tensor_is_refused() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    mut w: Tensor<f32, [2]> = [1.0, 2.0]
    val r = &mut w
    w -= 1.0f32
    r[0] = 5.0f32
    return 0
}
"#,
    );
    assert!(
        matches!(
            errors.as_slice(),
            [TypeError::CannotAssignWhileBorrowed { .. }]
        ),
        "got {errors:?}"
    );
}

const METHOD: &str = r#"
struct Weighted { scale: f32 }
impl Weighted {
    @grad
    func loss(&self, w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
        Tensor::scalar(w.sum() * self.scale)
    }
}
"#;

fn method_errors(body: &str) -> Vec<TypeError> {
    semantic_errors(&format!(
        "{METHOD}\nfunc main() -> i32 {{\n    val weighted = Weighted {{ scale: 2.0 }}\n{body}\n    return 0\n}}\n"
    ))
}

/// The AD scope rule holds for a method call's result as for a function's.
#[test]
fn a_backward_on_a_method_calls_result_is_accepted_and_ends_the_borrow() {
    let errors = method_errors(
        r#"
    mut w: Tensor<f32, [2]> = [1.0, 2.0]
    val l = weighted.loss(&mut w)
    l.backward()
    w -= 0.1f32 * w.grad()"#,
    );
    assert!(errors.is_empty(), "got {errors:?}");
}

#[test]
fn a_method_calls_differentiated_argument_is_borrowed_until_the_backward() {
    let errors = method_errors(
        r#"
    mut w: Tensor<f32, [2]> = [1.0, 2.0]
    val l = weighted.loss(&mut w)
    val r = &w
    l.backward()"#,
    );
    assert!(
        matches!(
            errors.as_slice(),
            [TypeError::CannotBorrowWhileMutablyBorrowed { .. }]
        ),
        "got {errors:?}"
    );
}

/// Under rule 3 the receiver is a constant, borrowed for the call alone.
#[test]
fn the_receiver_is_not_held_until_the_backward() {
    let errors = method_errors(
        r#"
    mut w: Tensor<f32, [2]> = [1.0, 2.0]
    val l = weighted.loss(&mut w)
    val again = &weighted
    l.backward()"#,
    );
    assert!(errors.is_empty(), "got {errors:?}");
}

#[test]
fn a_backward_in_another_block_than_the_method_call_is_refused() {
    let errors = method_errors(
        r#"
    mut w: Tensor<f32, [2]> = [1.0, 2.0]
    val l = weighted.loss(&mut w)
    if true {
        l.backward()
    }"#,
    );
    assert!(
        matches!(errors.as_slice(), [TypeError::BackwardUnavailable { .. }]),
        "got {errors:?}"
    );
}

const SELECTIVE: &str = r#"
struct Net { export w: Tensor<f32, [2]> }
impl Net {
    @grad(wrt: [self.w, k])
    func loss(&mut self, k: &mut Tensor<f32, [2]>, f: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
        Tensor::scalar(self.w.sum() + k.sum() + f.sum())
    }
}
"#;

fn selective_errors(body: &str) -> Vec<TypeError> {
    semantic_errors(&format!(
        "{SELECTIVE}\nfunc main() -> i32 {{\n    mut net = Net {{ w: [1.0, 2.0] }}\n    mut k: Tensor<f32, [2]> = [1.0, 2.0]\n    mut f: Tensor<f32, [2]> = [1.0, 2.0]\n    val l = net.loss(&mut k, &mut f)\n{body}\n    return 0\n}}\n"
    ))
}

/// A `wrt:` path reaching through the receiver makes `.backward()` write through it, so
/// the receiver is held like a differentiated argument.
#[test]
fn a_receiver_a_wrt_path_reaches_through_is_borrowed_until_the_backward() {
    let errors = selective_errors("    val r = &net\n    l.backward()");
    assert!(
        matches!(
            errors.as_slice(),
            [TypeError::CannotBorrowWhileMutablyBorrowed { .. }]
        ),
        "got {errors:?}"
    );
    let errors = selective_errors("    l.backward()\n    val g = net.w.grad()\n    val r = &k");
    assert!(errors.is_empty(), "got {errors:?}");
}

/// What `wrt:` leaves out is a constant: its borrow ends at the call, even a `&mut` one.
#[test]
fn an_argument_wrt_leaves_out_is_not_held() {
    let errors = selective_errors("    val r = &f\n    l.backward()");
    assert!(errors.is_empty(), "got {errors:?}");
    let errors = selective_errors("    val r = &k\n    l.backward()");
    assert!(
        matches!(
            errors.as_slice(),
            [TypeError::CannotBorrowWhileMutablyBorrowed { .. }]
        ),
        "got {errors:?}"
    );
}
