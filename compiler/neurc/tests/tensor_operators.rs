// By-value tensor operators (Phase 2C): `a + b`, `&a + &b`, the scalar broadcast, and
// the shape broadcast rule they share with in-place compound assignment, end to
// end through `neurc compile` and the linked binary.
//
// Unlike the compound assignment these ALLOCATE: each one builds a fresh tensor and
// leaves its operands' buffers to be released. Elements are read back by indexing, which
// 2B's slicing item made available, so the assertions are on values rather than on a
// panicking guard.
mod compile_harness;

use compile_harness::CompileTest;

fn run_program(name: &str, source: &str) -> i32 {
    CompileTest::new()
        .compile_and_run(name, source)
        .unwrap_or_else(|e| panic!("{name} should compile and run: {e}"))
}

#[test]
fn equal_shaped_operands_add_element_by_element() {
    let exit = run_program(
        "tensor_binary_add.nr",
        r#"
func main() -> i32 {
    val a: Tensor<i32, [2, 3]> = [[1, 2, 3], [4, 5, 6]]
    val b: Tensor<i32, [2, 3]> = [[10, 20, 30], [40, 50, 60]]
    val sum = a + b
    return sum[1, 2] - sum[0, 0]
}
"#,
    );
    assert_eq!(exit, 55);
}

#[test]
fn every_arithmetic_operator_runs_element_wise() {
    let exit = run_program(
        "tensor_binary_family.nr",
        r#"
func main() -> i32 {
    val a: Tensor<i32, [2]> = [12, 9]
    val b: Tensor<i32, [2]> = [4, 2]
    val sum = &a + &b
    val diff = &a - &b
    val prod = &a * &b
    val quot = &a / &b
    val rem = &a % &b
    return sum[0] + diff[0] + prod[0] + quot[0] + rem[1]
}
"#,
    );
    assert_eq!(exit, 16 + 8 + 48 + 3 + 1);
}

/// The operator allocates its result, so a borrowed operand is only read: the same two
/// bindings feed a second operator and are still readable afterwards.
#[test]
fn borrowed_operands_survive_the_operator() {
    let exit = run_program(
        "tensor_binary_borrowed.nr",
        r#"
func main() -> i32 {
    val a: Tensor<i32, [3]> = [1, 2, 3]
    val b: Tensor<i32, [3]> = [10, 10, 10]
    val sum = &a + &b
    val prod = &a * &b
    return sum[2] + prod[0] + a[1] + b[0]
}
"#,
    );
    assert_eq!(exit, 13 + 10 + 2 + 10);
}

#[test]
fn a_lower_rank_operand_is_repeated_across_the_leading_axes() {
    let exit = run_program(
        "tensor_binary_row_broadcast.nr",
        r#"
func main() -> i32 {
    val m: Tensor<i32, [2, 3]> = [[1, 2, 3], [4, 5, 6]]
    val row: Tensor<i32, [3]> = [100, 200, 300]
    val wide = &m + &row
    return (wide[0, 0] + wide[1, 2]) % 256
}
"#,
    );
    assert_eq!(exit, (101 + 306) % 256);
}

#[test]
fn a_size_one_axis_is_stretched_across_the_result() {
    let exit = run_program(
        "tensor_binary_column_broadcast.nr",
        r#"
func main() -> i32 {
    val m: Tensor<i32, [2, 3]> = [[1, 2, 3], [4, 5, 6]]
    val col: Tensor<i32, [2, 1]> = [[10], [20]]
    val tall = &m + &col
    return tall[0, 2] + tall[1, 0]
}
"#,
    );
    assert_eq!(exit, 13 + 24);
}

/// Both operands stretch at once: a `[1, 3]` row against a `[2, 1]` column produces the
/// full `[2, 3]` outer combination, which is the case a single-sided rule would miss.
#[test]
fn two_operands_stretch_into_one_result() {
    let exit = run_program(
        "tensor_binary_outer.nr",
        r#"
func main() -> i32 {
    val row: Tensor<i32, [1, 3]> = [[1, 2, 3]]
    val col: Tensor<i32, [2, 1]> = [[10], [20]]
    val grid = &row * &col
    return grid[0, 0] + grid[1, 2]
}
"#,
    );
    assert_eq!(exit, 10 + 60);
}

#[test]
fn a_scalar_broadcasts_from_either_side() {
    let exit = run_program(
        "tensor_binary_scalar.nr",
        r#"
func main() -> i32 {
    val m: Tensor<i32, [2, 2]> = [[1, 2], [3, 4]]
    val scaled = &m * 3
    val offset = 10 + m
    return scaled[1, 1] + offset[0, 0]
}
"#,
    );
    assert_eq!(exit, 12 + 11);
}

#[test]
fn a_float_scalar_broadcast_keeps_the_element_type() {
    let exit = run_program(
        "tensor_binary_float_scalar.nr",
        r#"
func main() -> i32 {
    val m: Tensor<f32, [2, 2]> = [[1.0, 2.0], [3.0, 4.0]]
    val scaled = &m * 2.0
    val shrunk = 0.5 * m
    val doubled = scaled[1, 1] as i32
    val halved = (shrunk[1, 1] * 10.0f32) as i32
    return doubled + halved
}
"#,
    );
    assert_eq!(exit, 8 + 20);
}

/// The scalar guards ride along: a tensor's arithmetic is its element's arithmetic, so a
/// division by zero aborts exactly where the scalar one would.
#[test]
fn an_element_division_by_zero_panics() {
    let result = CompileTest::new().compile_and_run(
        "tensor_binary_div_zero.nr",
        r#"
func main() -> i32 {
    val a: Tensor<i32, [2]> = [1, 2]
    val zero: Tensor<i32, [2]> = [1, 0]
    val quot = &a / &zero
    return quot[1]
}
"#,
    );
    let exit = result.expect("the program should compile and run");
    assert_ne!(exit, 0, "a divide-by-zero guard should stop the program");
}

/// A rank-0 tensor has one element and no axis, so the loop runs once and the
/// coordinate arithmetic has nothing to decompose.
#[test]
fn rank_zero_operands_combine() {
    let exit = run_program(
        "tensor_binary_rank_zero.nr",
        r#"
func main() -> i32 {
    val a = Tensor::<i32, []>::scalar(20)
    val b = Tensor::<i32, []>::scalar(22)
    val sum = a + b
    return sum.sum()
}
"#,
    );
    assert_eq!(exit, 42);
}

/// Compound assignment takes the by-value operator's broadcast rules, with the
/// result written back into the target's own buffer.
#[test]
fn compound_assignment_broadcasts_a_scalar_and_a_row() {
    let exit = run_program(
        "tensor_compound_broadcast.nr",
        r#"
func main() -> i32 {
    mut w: Tensor<i32, [2, 3]> = [[1, 2, 3], [4, 5, 6]]
    val row: Tensor<i32, [3]> = [10, 10, 10]
    w *= 2
    w += &row
    return w[0, 0] + w[1, 2]
}
"#,
    );
    assert_eq!(exit, 12 + 22);
}

/// A chain builds a temporary and consumes it in the next operator, so the intermediate
/// buffer is released by the operator that reads it rather than outliving the statement.
#[test]
fn a_chained_operator_consumes_its_own_temporary() {
    let exit = run_program(
        "tensor_binary_chain.nr",
        r#"
func main() -> i32 {
    val a: Tensor<i32, [3]> = [1, 2, 3]
    val b: Tensor<i32, [3]> = [1, 1, 1]
    val c: Tensor<i32, [3]> = [2, 2, 2]
    val chained = a + b + c
    return chained[0] + chained[2]
}
"#,
    );
    assert_eq!(exit, 4 + 6);
}
