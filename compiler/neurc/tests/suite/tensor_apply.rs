// Functional tensor operations (Phase 2F): `.map(f)`, `.zip(other, f)`,
// `.reduce(init, f)`, end to end through `neurc check` and `neurc compile`.
//
// Every runtime assertion reads values back rather than asserting a shape. A traversal
// that walked the wrong buffer, or called its function with the arguments the other way
// round, produces a result of exactly the right type, so only the numbers tell them apart.
//
// An exit code is one byte, so each expected value is kept below 256.

use crate::compile_harness::CompileTest;

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
fn a_map_rewrites_every_element_and_keeps_the_shape() {
    let source = r#"
func main() -> i32 {
    val t: Tensor<i32, [2, 3]> = [
        [1, 2, 3],
        [4, 5, 6]
    ]
    val doubled: Tensor<i32, [2, 3]> = t.map(|x: i32| -> i32 { x * 2 })
    // Row-major order is preserved, so the corners stay corners.
    return doubled[0, 0] + doubled[0, 2] * 2 + doubled[1, 0] * 3 + doubled[1, 2] * 4
}
"#;
    assert_eq!(
        run_program("apply_map.nr", source),
        2 + 6 * 2 + 8 * 3 + 12 * 4
    );
}

/// The result's element type is the FUNCTION's return type, not the receiver's element
/// type: `.map` is the one tensor construct that changes what a buffer holds.
#[test]
fn a_map_may_change_the_element_type() {
    let source = r#"
func main() -> i32 {
    val t: Tensor<i32, [4]> = [1, 2, 3, 4]
    val halves: Tensor<f64, [4]> = t.map(|x: i32| -> f64 { x as f64 / 2.0 })
    return (halves[3] * 10.0) as i32
}
"#;
    assert_eq!(run_program("apply_map_retype.nr", source), 20);
}

#[test]
fn a_zip_walks_both_buffers_at_one_index() {
    let source = r#"
func main() -> i32 {
    val a: Tensor<i32, [2, 2]> = [
        [1, 2],
        [3, 4]
    ]
    val b: Tensor<i32, [2, 2]> = [
        [5, 6],
        [7, 8]
    ]
    val paired: Tensor<i32, [2, 2]> = a.zip(b, |x: i32, y: i32| -> i32 { x * y })
    return paired[0, 0] + paired[0, 1] + paired[1, 0] + paired[1, 1]
}
"#;
    assert_eq!(run_program("apply_zip.nr", source), 5 + 12 + 21 + 32);
}

/// The two operands need not hold the same element type: the function's parameters name
/// what each buffer holds, and its return type names what the result holds.
#[test]
fn a_zip_may_pair_two_element_types() {
    let source = r#"
func main() -> i32 {
    val counts: Tensor<i32, [3]> = [1, 2, 3]
    val weights: Tensor<f64, [3]> = [0.5, 1.5, 2.0]
    val scaled: Tensor<f64, [3]> = counts.zip(weights, |n: i32, w: f64| -> f64 { n as f64 * w })
    return (scaled[0] * 10.0 + scaled[1] * 10.0 + scaled[2] * 10.0) as i32
}
"#;
    assert_eq!(run_program("apply_zip_mixed.nr", source), 5 + 30 + 60);
}

/// A fold yields ONE value of the seed's type, the way a whole-tensor `.sum()` does, not
/// a rank-0 tensor: a reader should not need a second call to get the number out.
#[test]
fn a_reduce_folds_the_whole_buffer_into_a_scalar() {
    let source = r#"
func main() -> i32 {
    val t: Tensor<i32, [2, 3]> = [
        [1, 2, 3],
        [4, 5, 6]
    ]
    val total: i32 = t.reduce(0, |acc: i32, x: i32| -> i32 { acc + x })
    return total
}
"#;
    assert_eq!(run_program("apply_reduce.nr", source), 21);
}

/// The accumulator is handed over FIRST, which is what `|acc, x|` means. A fold that
/// passed them the other way round would still type-check when both are `i32`, so the
/// operation here is deliberately non-commutative.
#[test]
fn a_fold_carries_its_accumulator_in_the_first_parameter() {
    let source = r#"
func main() -> i32 {
    val t: Tensor<i32, [3]> = [1, 2, 3]
    // ((10 * 2 + 1) * 2 + 2) * 2 + 3 = 91 left-folded; 10 the other way round differs.
    return t.reduce(10, |acc: i32, x: i32| -> i32 { acc * 2 + x })
}
"#;
    assert_eq!(run_program("apply_reduce_order.nr", source), 91);
}

/// Every traversal READS its operands. A borrowed receiver is accepted and the caller's
/// tensor survives the call, which is what makes a traversal usable on a shared weight.
#[test]
fn a_traversal_reads_its_operands_through_a_borrow() {
    let source = r#"
func scaled(w: &Tensor<i32, [2, 2]>, k: i32) -> Tensor<i32, [2, 2]> {
    return w.map(|x: i32| -> i32 { x * k })
}

func main() -> i32 {
    val w: Tensor<i32, [2, 2]> = [
        [1, 2],
        [3, 4]
    ]
    val s = scaled(&w, 5)
    // `w` is still readable here: nothing was moved into the traversal.
    return s[1, 1] + w[1, 1]
}
"#;
    assert_eq!(run_program("apply_borrowed.nr", source), 20 + 4);
}

/// Traversals compose: a `.map` produces an ordinary tensor value, so the next call in
/// the chain receives it as a receiver and the intermediate is released, not leaked.
#[test]
fn traversals_chain_through_their_results() {
    let source = r#"
func main() -> i32 {
    val t: Tensor<i32, [4]> = [1, 2, 3, 4]
    return t
        .map(|x: i32| -> i32 { x + 1 })
        .map(|x: i32| -> i32 { x * x })
        .reduce(0, |acc: i32, x: i32| -> i32 { acc + x })
}
"#;
    assert_eq!(run_program("apply_chain.nr", source), 4 + 9 + 16 + 25);
}

/// The function is an ordinary function VALUE, so anything that produces one works:
/// a closure literal, a closure binding, and a `>>` composition alike.
#[test]
fn a_composed_function_is_a_traversal_stage() {
    let source = r#"
func halve(x: i32) -> i32 { x / 2 }
func bump(x: i32) -> i32 { x + 1 }

func main() -> i32 {
    val t: Tensor<i32, [4]> = [2, 4, 6, 8]
    val rescale = halve >> bump
    return t.map(rescale).reduce(0, |acc: i32, x: i32| -> i32 { acc + x })
}
"#;
    assert_eq!(run_program("apply_composed.nr", source), 2 + 3 + 4 + 5);
}

/// An untyped seed takes its type from the accumulator parameter, which is the only thing
/// in the call that says whether a fold over an `f32` tensor carries `f32` or `f64`.
#[test]
fn an_untyped_seed_takes_the_accumulator_parameter_type() {
    let source = r#"
func main() -> i32 {
    val t: Tensor<f32, [3]> = [1.5, 2.5, 3.0]
    val total: f32 = t.reduce(0.0, |acc: f32, x: f32| -> f32 { acc + x })
    return (total * 10.0) as i32
}
"#;
    assert_eq!(run_program("apply_seed_type.nr", source), 70);
}

#[test]
fn a_non_function_argument_is_rejected() {
    let source = r#"
func main() -> i32 {
    val t: Tensor<i32, [2]> = [1, 2]
    val m = t.map(5)
    return 0
}
"#;
    let diagnostics = rejection("apply_not_callable.nr", source);
    assert!(
        diagnostics.contains("`.map` takes a function"),
        "unexpected diagnostics: {diagnostics}"
    );
}

#[test]
fn a_function_over_the_wrong_element_type_is_rejected() {
    let source = r#"
func main() -> i32 {
    val t: Tensor<i32, [2]> = [1, 2]
    val m = t.map(|x: f32| -> f32 { x })
    return 0
}
"#;
    let diagnostics = rejection("apply_param_type.nr", source);
    assert!(
        diagnostics.contains("parameter 0 of its function"),
        "unexpected diagnostics: {diagnostics}"
    );
}

#[test]
fn a_function_of_the_wrong_arity_is_rejected() {
    let source = r#"
func main() -> i32 {
    val t: Tensor<i32, [2]> = [1, 2]
    val m = t.map(|x: i32, y: i32| -> i32 { x })
    return 0
}
"#;
    let diagnostics = rejection("apply_arity.nr", source);
    assert!(
        diagnostics.contains("calls its function with 1 argument"),
        "unexpected diagnostics: {diagnostics}"
    );
}

/// One index walks both buffers, so two shapes would read past the shorter one. The
/// check is on the extents, which is why it is a type error rather than a bounds check.
#[test]
fn a_zip_over_two_shapes_is_rejected() {
    let source = r#"
func main() -> i32 {
    val a: Tensor<i32, [2, 2]> = [[1, 2], [3, 4]]
    val b: Tensor<i32, [3]> = [1, 2, 3]
    val z = a.zip(b, |x: i32, y: i32| -> i32 { x + y })
    return 0
}
"#;
    let diagnostics = rejection("apply_zip_shape.nr", source);
    assert!(
        diagnostics.contains("`.zip` walks two tensors at the same index"),
        "unexpected diagnostics: {diagnostics}"
    );
}

#[test]
fn a_zip_over_a_non_tensor_is_rejected() {
    let source = r#"
func main() -> i32 {
    val a: Tensor<i32, [2]> = [1, 2]
    val z = a.zip(7, |x: i32, y: i32| -> i32 { x + y })
    return 0
}
"#;
    let diagnostics = rejection("apply_zip_operand.nr", source);
    assert!(
        diagnostics.contains("`.zip` takes a tensor"),
        "unexpected diagnostics: {diagnostics}"
    );
}

/// A fold's function answers the next accumulator, so its return type is the seed's.
#[test]
fn a_fold_answering_a_type_its_seed_cannot_carry_is_rejected() {
    let source = r#"
func main() -> i32 {
    val t: Tensor<i32, [2]> = [1, 2]
    val total = t.reduce(0, |acc: i32, x: i32| -> f64 { 1.0 })
    return 0
}
"#;
    let diagnostics = rejection("apply_reduce_acc.nr", source);
    assert!(
        diagnostics.contains("must answer the seed's type"),
        "unexpected diagnostics: {diagnostics}"
    );
}

/// A tensor holds numbers, so a `.map` that answers something else has nowhere to put
/// its results. `.reduce` has no such rule: its answer never enters a buffer.
#[test]
fn a_map_answering_a_non_element_type_is_rejected() {
    let source = r#"
func main() -> i32 {
    val t: Tensor<i32, [2]> = [1, 2]
    val m = t.map(|x: i32| -> bool { x > 0 })
    return 0
}
"#;
    let diagnostics = rejection("apply_result_element.nr", source);
    assert!(
        diagnostics.contains("requires an integer or `f32`/`f64`"),
        "unexpected diagnostics: {diagnostics}"
    );
}

/// There is deliberately no `.filter`: its output length depends on the values in the
/// buffer, so the result would have no shape the type system can name.
#[test]
fn a_tensor_has_no_filter() {
    let source = r#"
func main() -> i32 {
    val t: Tensor<i32, [3]> = [1, 2, 3]
    val kept = t.filter(|x: i32| -> bool { x > 1 })
    return 0
}
"#;
    let diagnostics = rejection("apply_no_filter.nr", source);
    assert!(
        diagnostics.contains("has no method 'filter'"),
        "unexpected diagnostics: {diagnostics}"
    );
}

/// The traversal's length and its result buffer are both built from the extents, so a
/// shape parameter is rejected on the template, exactly as a contraction's is.
#[test]
fn a_traversal_over_a_shape_generic_tensor_is_rejected() {
    let source = r#"
func twice<N>(t: &Tensor<i32, [N]>) -> Tensor<i32, [N]> {
    return t.map(|x: i32| -> i32 { x * 2 })
}

func main() -> i32 {
    val t: Tensor<i32, [2]> = [1, 2]
    val m = twice(&t)
    return m[0]
}
"#;
    let diagnostics = rejection("apply_symbolic.nr", source);
    assert!(
        diagnostics.contains("`.map` needs every extent of the receiver to be known here"),
        "unexpected diagnostics: {diagnostics}"
    );
}
