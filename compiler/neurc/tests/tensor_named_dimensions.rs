// Named tensor dimensions (Phase 2B): `Tensor<f32, [batch: 32, embed: 768]>`
// end to end through `neurc check` and `neurc compile`.
//
// The names are a frontend-only annotation, so the value assertions here also prove the
// erasure: a named shape reaches the backend as the shape it always was, and the program
// computes the same answer it would have without the names.
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
fn a_named_shape_compiles_and_runs_like_the_unnamed_one() {
    let source = r#"
func width(image: &Tensor<i32, [channels: 3, height: 2, width: 4]>) -> i32 {
    return image[0, 1, 3]
}

func main() -> i32 {
    val image: Tensor<i32, [channels: 3, height: 2, width: 4]> = Tensor::<i32, [3, 2, 4]>::ones()
    return width(&image) + 41
}
"#;
    assert_eq!(run_program("named_shape.nr", source), 42);
}

/// The names are not part of type identity beyond the both-named rule, so a function
/// written against a bare shape still takes a named tensor and vice versa.
#[test]
fn a_named_tensor_passes_to_an_unnamed_parameter() {
    let source = r#"
func sum(t: &Tensor<i32, [2, 2]>) -> i32 {
    return t[0, 0] + t[0, 1] + t[1, 0] + t[1, 1]
}

func main() -> i32 {
    val t: Tensor<i32, [rows: 2, cols: 2]> = [
        [1, 2],
        [3, 4]
    ]
    return sum(&t)
}
"#;
    assert_eq!(run_program("named_to_unnamed.nr", source), 10);
}

/// The reason the feature exists: identical extents, transposed names.
#[test]
fn a_transposed_argument_is_rejected_with_the_axis_named() {
    let source = r#"
func normalize(x: Tensor<f32, [height: 4, width: 4]>) { }

func main() -> i32 {
    val t: Tensor<f32, [width: 4, height: 4]> = Tensor::<f32, [4, 4]>::zeros()
    normalize(t)
    return 0
}
"#;
    let out = rejection("transposed_axes.nr", source);
    assert!(
        out.contains("axis 0") && out.contains("height") && out.contains("width"),
        "the diagnostic should name the disagreeing axis; got: {out}"
    );
}

#[test]
fn a_repeated_dimension_name_is_rejected() {
    let source = r#"
func square(x: Tensor<f32, [side: 4, side: 4]>) { }

func main() -> i32 {
    return 0
}
"#;
    let out = rejection("repeated_axis_name.nr", source);
    assert!(
        out.contains("side"),
        "the diagnostic should name the repeated dimension; got: {out}"
    );
}

/// A slice keeps the names of the axes that survive it, and drops the rest with them.
#[test]
fn a_slice_keeps_the_surviving_axis_name() {
    let source = r#"
func widest(row: &Tensor<i32, [width: 3]>) -> i32 {
    return row[0] + row[1] + row[2]
}

func main() -> i32 {
    val plane: Tensor<i32, [height: 2, width: 3]> = [
        [1, 2, 3],
        [4, 5, 6]
    ]
    val row: Tensor<i32, [width: 3]> = plane[1, ..]
    return widest(&row)
}
"#;
    assert_eq!(run_program("slice_keeps_name.nr", source), 15);
}

/// A dimension name and a shape parameter occupy different namespaces: the name is the
/// axis's, the extent is the function's `const N: u32`.
#[test]
fn a_named_axis_carries_a_shape_parameter_as_its_extent() {
    let source = r#"
func length<W>(row: &Tensor<i32, [width: W]>) -> i32 {
    return W as i32
}

func main() -> i32 {
    val row: Tensor<i32, [width: 5]> = Tensor::<i32, [5]>::zeros()
    return length(&row)
}
"#;
    assert_eq!(run_program("named_shape_param.nr", source), 5);
}
