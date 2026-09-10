// End-to-end tests for tensor shape generics and shape constraints.
//
// A bare name in a tensor shape is a compile-time `const NAME: u32` parameter, inferred
// from the argument tensors and monomorphized per distinct set of extents. A repeated
// parameter must agree everywhere it is written, and a `where` predicate over one is
// checked at the call that supplies the offending extent. These tests drive the whole
// pipeline (parse → type-check → HIR lowering → LLVM → native binary) and assert on the
// program's exit code.
mod common;
use common::CompileTest;

#[test]
fn shape_parameters_are_inferred_from_the_argument() {
    let test = CompileTest::new();
    // M and K come from the argument's own shape; the body reads one element.
    let source = r#"
func corner<M, K>(t: &Tensor<i32, [M, K]>) -> i32 {
    return t[0, 0]
}

func main() -> i32 {
    val grid: Tensor<i32, [2, 3]> = [[7, 2, 3], [4, 5, 6]]
    return corner(&grid)
}
"#;
    let exit = test
        .compile_and_run("shape_infer.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 7);
}

#[test]
fn distinct_shapes_monomorphize_separately() {
    let test = CompileTest::new();
    // Two calls at different extents produce two instances of the same template.
    let source = r#"
func rank_two_size<M, K>(t: &Tensor<i32, [M, K]>) -> i32 {
    return (M * K) as i32
}

func main() -> i32 {
    val wide: Tensor<i32, [2, 3]> = [[1, 2, 3], [4, 5, 6]]
    val tall: Tensor<i32, [4, 2]> = Tensor::<i32, [4, 2]>::zeros()
    return rank_two_size(&wide) + rank_two_size(&tall)
}
"#;
    let exit = test
        .compile_and_run("shape_mono.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 14);
}

#[test]
fn a_shape_parameter_reaches_the_return_type() {
    let test = CompileTest::new();
    // The returned row is `Tensor<i32, [K]>`, so the caller's annotation names the
    // extent K was inferred to be.
    let source = r#"
func top_row<M, K>(t: &Tensor<i32, [M, K]>) -> Tensor<i32, [K]> {
    return t[0, ..]
}

func main() -> i32 {
    val grid: Tensor<i32, [2, 3]> = [[10, 20, 30], [40, 50, 60]]
    val row: Tensor<i32, [3]> = top_row(&grid)
    return row[0] + row[2]
}
"#;
    let exit = test
        .compile_and_run("shape_return.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 40);
}

#[test]
fn a_shape_generic_body_updates_a_tensor_in_place() {
    let test = CompileTest::new();
    // The template is written once and instantiated at two widths; the in-place
    // `-=` inside it operates on whatever extent N turned out to be.
    let source = r#"
func decay<N>(a: Tensor<i32, [N]>, step: &Tensor<i32, [N]>) -> Tensor<i32, [N]>
where N > 0
{
    mut out = a
    out -= step
    return out
}

func main() -> i32 {
    val short: Tensor<i32, [2]> = [10, 20]
    val short_step: Tensor<i32, [2]> = [1, 2]
    val long: Tensor<i32, [3]> = [30, 40, 50]
    val long_step: Tensor<i32, [3]> = [3, 4, 5]

    val a = decay(short, &short_step)
    val b = decay(long, &long_step)
    return a[1] + b[2]
}
"#;
    let exit = test
        .compile_and_run("shape_in_place.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 63);
}

#[test]
fn an_explicit_const_parameter_works_in_a_shape_position() {
    let test = CompileTest::new();
    // The bare name is sugar for this spelling, so both must name the same parameter.
    let source = r#"
func width<const K: u32>(t: &Tensor<i32, [K]>) -> i32 {
    return K as i32
}

func main() -> i32 {
    val v: Tensor<i32, [5]> = [1, 2, 3, 4, 5]
    return width(&v)
}
"#;
    let exit = test
        .compile_and_run("shape_explicit_const.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 5);
}

#[test]
fn a_repeated_shape_parameter_must_agree() {
    let test = CompileTest::new();
    let source = r#"
func pair<M, N, K>(a: &Tensor<i32, [M, K]>, b: &Tensor<i32, [K, N]>) -> i32 {
    return a[0, 0] + b[0, 0]
}

func main() -> i32 {
    val x: Tensor<i32, [2, 3]> = [[1, 2, 3], [4, 5, 6]]
    val y: Tensor<i32, [5, 6]> = Tensor::<i32, [5, 6]>::zeros()
    return pair(&x, &y)
}
"#;
    let err = test
        .check("shape_conflict.nr", source)
        .expect_err("a contradicted shape parameter must be rejected");
    assert!(
        err.contains("shape parameter 'K'") && err.contains("3") && err.contains("5"),
        "expected the conflict to name K and both extents; got {err}"
    );
}

#[test]
fn a_shape_predicate_is_checked_at_the_call() {
    let test = CompileTest::new();
    let source = r#"
func wide<N>(t: &Tensor<i32, [N]>) -> i32 where N > 2 {
    return t[0]
}

func main() -> i32 {
    val pair: Tensor<i32, [2]> = [1, 2]
    return wide(&pair)
}
"#;
    let err = test
        .check("shape_predicate.nr", source)
        .expect_err("a violated predicate must be rejected");
    assert!(
        err.contains("predicate"),
        "expected the predicate to be reported; got {err}"
    );
}

#[test]
fn an_undeclared_dimension_is_named() {
    let test = CompileTest::new();
    let source = r#"
func read(t: &Tensor<i32, [Q]>) -> i32 {
    return t[0]
}

func main() -> i32 {
    return 0
}
"#;
    let err = test
        .check("shape_undeclared.nr", source)
        .expect_err("an undeclared extent must be rejected");
    assert!(
        err.contains("'Q'"),
        "expected the dimension to be named; got {err}"
    );
}
