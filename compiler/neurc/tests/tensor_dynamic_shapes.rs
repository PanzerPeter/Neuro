// Dynamic tensor shapes (Phase 2B): the `?` axis of `Tensor<f32, [?, 784]>` end to end
// through `neurc check` and `neurc compile`.
//
// A `?` axis has no compile-time extent, so it is an ACCEPTING position: one function
// takes every batch size, while the axes beside it stay checked exactly as before. The
// value assertions prove the run-time half of that — a widened tensor is still the same
// DLPack handle addressing the same buffer, so it moves, is stored, and is released with
// no extent involved.
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
fn one_dynamic_parameter_accepts_every_extent_at_that_axis() {
    let source = r#"
func columns(batch: &Tensor<i32, [?, 4]>) -> i32 {
    return 4
}

func main() -> i32 {
    val two: Tensor<i32, [2, 4]> = [
        [1, 2, 3, 4],
        [5, 6, 7, 8]
    ]
    val seven = Tensor::<i32, [7, 4]>::zeros()
    return columns(&two) * columns(&seven) + 26
}
"#;
    assert_eq!(run_program("dynamic_parameter.nr", source), 42);
}

/// The point of the axis being per-position: `?` waives the check on its own axis and
/// on no other.
#[test]
fn a_static_axis_beside_a_dynamic_one_is_still_a_compile_error() {
    let source = r#"
func columns(batch: &Tensor<i32, [?, 4]>) -> i32 {
    return 4
}

func main() -> i32 {
    val wrong = Tensor::<i32, [2, 8]>::zeros()
    return columns(&wrong)
}
"#;
    let errors = rejection("dynamic_static_axis.nr", source);
    assert!(
        errors.contains("[?, 4]") && errors.contains("[2, 8]"),
        "the mismatch names both shapes: {errors}"
    );
}

/// A dynamic tensor is a value like any other: it crosses a call boundary by move, is
/// bound, and is released at scope exit without its extents ever being needed.
#[test]
fn a_widened_tensor_moves_and_is_released() {
    let source = r#"
func widen(t: Tensor<f32, [3, 2]>) -> Tensor<f32, [?, 2]> {
    return t
}

func main() -> i32 {
    val source: Tensor<f32, [3, 2]> = [
        [1.0, 2.0],
        [3.0, 4.0],
        [5.0, 6.0]
    ]
    val batch = widen(source)
    println("a dynamic batch was built")
    return 21
}
"#;
    assert_eq!(run_program("dynamic_move.nr", source), 21);
}

/// Widening is sound and narrowing is not: the run-time shape may be anything, so a
/// static annotation cannot take a `?` back without a run-time check the language does
/// not have yet.
#[test]
fn a_dynamic_shape_does_not_satisfy_a_static_annotation() {
    let source = r#"
func widen(t: Tensor<f32, [3, 2]>) -> Tensor<f32, [?, 2]> {
    return t
}

func main() -> i32 {
    val narrowed: Tensor<f32, [3, 2]> = widen(Tensor::<f32, [3, 2]>::zeros())
    return 0
}
"#;
    let errors = rejection("dynamic_narrowing.nr", source);
    assert!(
        errors.contains("expected Tensor<f32, [3, 2]>"),
        "the annotation is the expectation that fails: {errors}"
    );
}

/// Everything that computes a buffer size, a stride or an element count is refused,
/// because a `?` has no value to compute it from.
#[test]
fn an_operation_needing_the_extent_is_refused() {
    let source = r#"
func flexible(x: Tensor<f32, [?, 4]>) -> i32 {
    val copied = x.clone()
    return 0
}

func main() -> i32 {
    return 0
}
"#;
    let errors = rejection("dynamic_clone.nr", source);
    assert!(
        errors.contains("an axis is `?`"),
        "the diagnostic names the dynamic axis: {errors}"
    );
}

/// A `?` axis carries a dimension name like any other, and the name rule is unchanged:
/// checked wherever both sides supply one.
#[test]
fn a_dynamic_axis_keeps_its_dimension_name() {
    let source = r#"
func rows(batch: &Tensor<i32, [batch: ?, embed: 3]>) -> i32 {
    return 3
}

func main() -> i32 {
    val t: Tensor<i32, [batch: 2, embed: 3]> = [
        [1, 2, 3],
        [4, 5, 6]
    ]
    return rows(&t) * 14
}
"#;
    assert_eq!(run_program("dynamic_named_axis.nr", source), 42);
}

/// A transposed name is still caught under a `?`: the extents were never what
/// distinguished those two shapes.
#[test]
fn a_transposed_name_is_caught_under_a_dynamic_extent() {
    let source = r#"
func rows(batch: &Tensor<i32, [batch: ?, embed: 3]>) -> i32 {
    return 3
}

func main() -> i32 {
    val t: Tensor<i32, [embed: 2, batch: 3]> = [
        [1, 2, 3],
        [4, 5, 6]
    ]
    return rows(&t)
}
"#;
    let errors = rejection("dynamic_transposed_name.nr", source);
    assert!(
        errors.contains("batch") && errors.contains("embed"),
        "the names are what disagree: {errors}"
    );
}
