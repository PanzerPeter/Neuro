// Einstein notation (Phase 2F): `einsum("bij,bjk->bik", a, b)`, end to end through
// `neurc check` and `neurc compile`.
//
// Every runtime assertion reads the contracted values back rather than asserting a
// shape. A contraction that gathered the wrong run produces a result of exactly the
// right type: `einsum("ij,jk->ik", a, b)` and the same call with the operands swapped
// agree on the result's shape whenever the matrices are square, so only the numbers
// tell the two apart.
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
fn a_contraction_over_one_letter_is_a_matrix_product() {
    let source = r#"
func main() -> i32 {
    val a: Tensor<i32, [2, 3]> = [
        [1, 2, 3],
        [4, 5, 6]
    ]
    val b: Tensor<i32, [3, 2]> = [
        [1, 0],
        [0, 1],
        [1, 1]
    ]
    val m: Tensor<i32, [2, 2]> = einsum("ij,jk->ik", a, b)
    // m = ((1+3, 2+3), (4+6, 5+6)) = ((4, 5), (10, 11)).
    return m[0, 0] + m[0, 1] * 2 + m[1, 0] * 3 + m[1, 1] * 4
}
"#;
    assert_eq!(run_program("einsum_matmul.nr", source), 4 + 10 + 30 + 44);
}

/// A repeated letter within ONE operand walks its diagonal, which is the whole of the
/// trace. Nothing else in the language addresses a buffer that way.
#[test]
fn a_letter_repeated_in_one_operand_walks_its_diagonal() {
    let source = r#"
func main() -> i32 {
    val m: Tensor<i32, [3, 3]> = [
        [1, 2, 3],
        [4, 5, 6],
        [7, 8, 9]
    ]
    return einsum("ii->", m)
}
"#;
    assert_eq!(run_program("einsum_trace.nr", source), 15);
}

/// No letter is contracted here, so the inner loop runs exactly once per output slot.
#[test]
fn a_subscript_that_contracts_nothing_is_an_outer_product() {
    let source = r#"
func main() -> i32 {
    val u: Tensor<i32, [2]> = [3, 4]
    val v: Tensor<i32, [3]> = [1, 2, 5]
    val o: Tensor<i32, [2, 3]> = einsum("i,j->ij", u, v)
    // o = ((3, 6, 15), (4, 8, 20)).
    return o[0, 0] + o[0, 2] + o[1, 1] + o[1, 2]
}
"#;
    assert_eq!(run_program("einsum_outer.nr", source), 3 + 15 + 8 + 20);
}

/// Reordering the output letters permutes the result without touching any operand,
/// which is the one case where the output counter's digit layout does all the work.
#[test]
fn reordering_the_output_letters_transposes() {
    let source = r#"
func main() -> i32 {
    val a: Tensor<i32, [2, 3]> = [
        [1, 2, 3],
        [4, 5, 6]
    ]
    val t: Tensor<i32, [3, 2]> = einsum("ij->ji", a)
    return t[2, 1] * 10 + t[0, 1]
}
"#;
    assert_eq!(run_program("einsum_transpose.nr", source), 64);
}

/// An input letter missing from the output is summed over, so a one-operand
/// contraction reproduces an axis reduction.
#[test]
fn dropping_an_input_letter_sums_over_it() {
    let source = r#"
func main() -> i32 {
    val a: Tensor<i32, [2, 3]> = [
        [1, 2, 3],
        [4, 5, 6]
    ]
    val rows: Tensor<i32, [2]> = einsum("ij->i", a)
    val cols: Tensor<i32, [3]> = einsum("ij->j", a)
    // rows = (6, 15), cols = (5, 7, 9).
    return rows[0] + rows[1] * 2 + cols[0] + cols[2]
}
"#;
    assert_eq!(run_program("einsum_sum_axis.nr", source), 6 + 30 + 5 + 9);
}

/// A batch letter appearing in both operands and in the output is neither contracted
/// nor free of the operands: it indexes all three at once.
#[test]
fn a_shared_output_letter_batches_the_contraction() {
    let source = r#"
func main() -> i32 {
    val a: Tensor<i32, [2, 2, 3]> = [
        [[1, 2, 3], [4, 5, 6]],
        [[7, 8, 9], [1, 1, 1]]
    ]
    val b: Tensor<i32, [2, 3, 2]> = [
        [[1, 0], [0, 1], [1, 1]],
        [[2, 0], [0, 2], [1, 1]]
    ]
    val m: Tensor<i32, [2, 2, 2]> = einsum("bij,bjk->bik", a, b)
    // batch 0 = ((4, 5), (10, 11)); batch 1 = ((23, 25), (3, 3)).
    return m[0, 0, 0] + m[0, 1, 1] + m[1, 0, 0] + m[1, 1, 0]
}
"#;
    assert_eq!(run_program("einsum_batch.nr", source), 4 + 11 + 23 + 3);
}

/// Both operands read one letter and the output keeps none, so the result is a scalar
/// of the element type rather than a rank-0 tensor.
#[test]
fn an_empty_output_subscript_yields_the_element_type() {
    let source = r#"
func main() -> i32 {
    val u: Tensor<i32, [3]> = [1, 2, 3]
    val v: Tensor<i32, [3]> = [4, 5, 6]
    val dot: i32 = einsum("i,i->", u, v)
    return dot
}
"#;
    assert_eq!(run_program("einsum_dot.nr", source), 4 + 10 + 18);
}

/// A contraction READS its operands, so a borrowed one is accepted and the caller's
/// binding survives the call.
#[test]
fn a_borrowed_operand_is_read_not_moved() {
    let source = r#"
func weighted(w: &Tensor<f32, [2, 2]>, x: &Tensor<f32, [2]>) -> Tensor<f32, [2]> {
    einsum("ij,j->i", w, x)
}

func main() -> i32 {
    val w: Tensor<f32, [2, 2]> = [[1.0, 2.0], [3.0, 4.0]]
    val x: Tensor<f32, [2]> = [5.0, 6.0]
    val y = weighted(&w, &x)
    // y = (17.0, 39.0), and `w` is still usable afterwards.
    return (y[0] as i32) + (y[1] as i32) + (w[1, 1] as i32)
}
"#;
    assert_eq!(run_program("einsum_borrowed.nr", source), 17 + 39 + 4);
}

/// An operand built for the call owns a buffer no binding releases, so the contraction
/// has to free it. A loop turns a leak into an unbounded one, which the runtime shows.
#[test]
fn a_temporary_operand_is_released() {
    let source = r#"
func main() -> i32 {
    val w: Tensor<i32, [2, 2]> = [[1, 2], [3, 4]]
    mut total: i32 = 0
    mut step: i32 = 0
    while step < 1000 {
        val m = einsum("ij,jk->ik", &w + &w, &w)
        total = m[0, 0]
        step = step + 1
    }
    // (2w @ w)[0, 0] = 2 * (1 * 1 + 2 * 3) = 14.
    return total
}
"#;
    assert_eq!(run_program("einsum_temporary.nr", source), 14);
}

#[test]
fn a_float_contraction_multiplies_and_sums_in_the_element_type() {
    let source = r#"
func main() -> i32 {
    val a: Tensor<f32, [2, 2]> = [[0.5, 1.5], [2.5, 3.5]]
    val b: Tensor<f32, [2, 2]> = [[2.0, 0.0], [0.0, 2.0]]
    val m: Tensor<f32, [2, 2]> = einsum("ij,jk->ik", a, b)
    // m = ((1.0, 3.0), (5.0, 7.0)).
    return (m[0, 0] as i32) + (m[0, 1] as i32) * 2 + (m[1, 1] as i32) * 3
}
"#;
    assert_eq!(run_program("einsum_float.nr", source), 1 + 6 + 21);
}

/// A user function of the same name shadows the builtin, exactly as it shadows
/// `print` and the panic family.
#[test]
fn a_user_function_named_einsum_shadows_the_builtin() {
    let source = r#"
func einsum(n: i32) -> i32 {
    n * 2
}

func main() -> i32 {
    einsum(21)
}
"#;
    assert_eq!(run_program("einsum_shadowed.nr", source), 42);
}

#[test]
fn a_computed_subscript_string_is_rejected() {
    let source = r#"
func main() -> i32 {
    val a: Tensor<i32, [2, 2]> = [[1, 2], [3, 4]]
    val spec = "ij->i"
    val bad = einsum(spec, a)
    return 0
}
"#;
    let diagnostics = rejection("einsum_computed_spec.nr", source);
    assert!(
        diagnostics.contains("string literal"),
        "expected a literal-subscript diagnostic, got: {diagnostics}"
    );
}

#[test]
fn a_subscript_without_an_arrow_is_rejected() {
    let source = r#"
func main() -> i32 {
    val a: Tensor<i32, [2, 2]> = [[1, 2], [3, 4]]
    val bad = einsum("ij", a)
    return 0
}
"#;
    let diagnostics = rejection("einsum_no_arrow.nr", source);
    assert!(
        diagnostics.contains("no `->`"),
        "expected a missing-arrow diagnostic, got: {diagnostics}"
    );
}

#[test]
fn a_subscript_letter_that_is_not_a_letter_is_rejected() {
    let source = r#"
func main() -> i32 {
    val a: Tensor<i32, [2, 2]> = [[1, 2], [3, 4]]
    val bad = einsum("i1->i", a)
    return 0
}
"#;
    let diagnostics = rejection("einsum_digit.nr", source);
    assert!(
        diagnostics.contains("not a subscript letter"),
        "expected a subscript-letter diagnostic, got: {diagnostics}"
    );
}

#[test]
fn an_operand_count_that_disagrees_with_the_subscripts_is_rejected() {
    let source = r#"
func main() -> i32 {
    val a: Tensor<i32, [2, 2]> = [[1, 2], [3, 4]]
    val bad = einsum("ij,jk->ik", a)
    return 0
}
"#;
    let diagnostics = rejection("einsum_operand_count.nr", source);
    assert!(
        diagnostics.contains("comma-separated subscripts"),
        "expected an operand-count diagnostic, got: {diagnostics}"
    );
}

#[test]
fn a_subscript_of_the_wrong_length_is_rejected() {
    let source = r#"
func main() -> i32 {
    val a: Tensor<i32, [2, 2]> = [[1, 2], [3, 4]]
    val bad = einsum("ijk->i", a)
    return 0
}
"#;
    let diagnostics = rejection("einsum_rank.nr", source);
    assert!(
        diagnostics.contains("one letter per axis"),
        "expected a rank diagnostic, got: {diagnostics}"
    );
}

/// The specification requires the conflict diagnostic to name the letter, because the
/// letter is the only thing in the call that says which two axes were meant to agree.
#[test]
fn a_letter_bound_to_two_extents_is_rejected_by_name() {
    let source = r#"
func main() -> i32 {
    val a: Tensor<i32, [2, 3]> = [[1, 2, 3], [4, 5, 6]]
    val b: Tensor<i32, [2, 3]> = [[1, 2, 3], [4, 5, 6]]
    val bad = einsum("ij,ji->ij", a, b)
    return 0
}
"#;
    let diagnostics = rejection("einsum_extent_conflict.nr", source);
    assert!(
        diagnostics.contains("binds 'j' to extent 3 and then to 2"),
        "expected an extent-conflict diagnostic naming 'j', got: {diagnostics}"
    );
}

#[test]
fn an_output_letter_no_operand_binds_is_rejected_by_name() {
    let source = r#"
func main() -> i32 {
    val a: Tensor<i32, [2, 2]> = [[1, 2], [3, 4]]
    val bad = einsum("ij->iz", a)
    return 0
}
"#;
    let diagnostics = rejection("einsum_unbound_output.nr", source);
    assert!(
        diagnostics.contains("writes 'z' on the right of `->`"),
        "expected an unbound-letter diagnostic naming 'z', got: {diagnostics}"
    );
}

#[test]
fn a_repeated_output_letter_is_rejected_by_name() {
    let source = r#"
func main() -> i32 {
    val a: Tensor<i32, [2, 2]> = [[1, 2], [3, 4]]
    val bad = einsum("ij->ii", a)
    return 0
}
"#;
    let diagnostics = rejection("einsum_repeated_output.nr", source);
    assert!(
        diagnostics.contains("writes 'i' twice"),
        "expected a repeated-letter diagnostic naming 'i', got: {diagnostics}"
    );
}

#[test]
fn a_non_tensor_operand_is_rejected() {
    let source = r#"
func main() -> i32 {
    val x = 5
    val bad = einsum("i->i", x)
    return 0
}
"#;
    let diagnostics = rejection("einsum_not_a_tensor.nr", source);
    assert!(
        diagnostics.contains("not a tensor"),
        "expected a non-tensor diagnostic, got: {diagnostics}"
    );
}

#[test]
fn operands_of_different_element_types_are_rejected() {
    let source = r#"
func main() -> i32 {
    val a: Tensor<i32, [2, 2]> = [[1, 2], [3, 4]]
    val b: Tensor<f32, [2, 2]> = [[1.0, 2.0], [3.0, 4.0]]
    val bad = einsum("ij,jk->ik", a, b)
    return 0
}
"#;
    let diagnostics = rejection("einsum_element_mismatch.nr", source);
    assert!(
        diagnostics.contains("shares an element type"),
        "expected an element-type diagnostic, got: {diagnostics}"
    );
}

#[test]
fn a_bool_element_type_is_rejected() {
    let source = r#"
func main() -> i32 {
    val a: Tensor<bool, [2, 2]> = [[true, false], [false, true]]
    val bad = einsum("ij->i", a)
    return 0
}
"#;
    let diagnostics = rejection("einsum_bool.nr", source);
    assert!(
        diagnostics.contains("integer or float element type"),
        "expected an element-type diagnostic, got: {diagnostics}"
    );
}

/// A half-precision operand contracts like any float one, each product and sum computed in
/// `f32` and rounded back.
#[test]
fn a_half_precision_contraction_computes() {
    let test = crate::compile_harness::CompileTest::new();
    let source = r#"
func main() -> i32 {
    val m: Tensor<bf16, [2, 2]> = [[1.0bf16, 2.0bf16], [3.0bf16, 4.0bf16]]
    val t = einsum("ij,jk->ik", m, m)
    t[1, 1] as i32
}
"#;
    let exit = test
        .compile_and_run("einsum_bf16.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 22);
}
