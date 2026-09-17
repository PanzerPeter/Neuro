// Matrix multiplication (Phase 2C): the `@` operator, end to end through
// `neurc compile` and the linked binary.
//
// `@` is the one tensor operator that is not element-wise. It contracts the operands'
// inner axis — `[M, K] @ [K, N]` gives `[M, N]` — so unlike `+` it neither broadcasts
// nor accepts a scalar, and its precedence (Appendix B row 4) is tighter than `*` so
// that `a @ b * c` scales the product rather than multiplying by a scaled operand.

use crate::compile_harness::CompileTest;

fn run_program(name: &str, source: &str) -> i32 {
    CompileTest::new()
        .compile_and_run(name, source)
        .unwrap_or_else(|e| panic!("{name} should compile and run: {e}"))
}

#[test]
fn a_matrix_product_computes_every_element() {
    // [[1, 2, 3], [4, 5, 6]] @ [[7, 8], [9, 10], [11, 12]] = [[58, 64], [139, 154]]
    let exit = run_program(
        "matmul_elements.nr",
        r#"
func main() -> i32 {
    val a: Tensor<i32, [2, 3]> = [
        [1, 2, 3],
        [4, 5, 6]
    ]
    val b: Tensor<i32, [3, 2]> = [
        [7, 8],
        [9, 10],
        [11, 12]
    ]
    val c = &a @ &b
    if c[0, 0] != 58 {
        return 1
    }
    if c[0, 1] != 64 {
        return 2
    }
    if c[1, 0] != 139 {
        return 3
    }
    return c[1, 1] - 150
}
"#,
    );
    assert_eq!(exit, 4);
}

/// Multiplying by the identity leaves every element alone, which is the cheapest whole
/// matrix to assert on: one wrong accumulator anywhere changes a value.
#[test]
fn multiplying_by_the_identity_is_the_original() {
    let exit = run_program(
        "matmul_identity.nr",
        r#"
func main() -> i32 {
    val a: Tensor<f32, [3, 3]> = [
        [1.5, 2.5, 3.5],
        [4.5, 5.5, 6.5],
        [7.5, 8.5, 9.5]
    ]
    val i = Tensor::<f32, [3, 3]>::identity()
    val same = &a @ &i
    mut matched = 0
    for row in 0..3 {
        for column in 0..3 {
            if same[row, column] == a[row, column] {
                matched = matched + 1
            }
        }
    }
    return matched
}
"#,
    );
    assert_eq!(exit, 9);
}

/// A borrowed operand is read rather than consumed, so one weight feeds two products —
/// the reason every tensor operator is defined on `&T` as well.
#[test]
fn a_borrowed_operand_survives_the_product() {
    let exit = run_program(
        "matmul_borrowed.nr",
        r#"
func project(w: &Tensor<i32, [2, 2]>, x: &Tensor<i32, [2, 2]>) -> Tensor<i32, [2, 2]> {
    return w @ x
}

func main() -> i32 {
    val w: Tensor<i32, [2, 2]> = [[1, 2], [3, 4]]
    val x: Tensor<i32, [2, 2]> = [[1, 0], [0, 1]]
    val first = project(&w, &x)
    val second = project(&w, &x)
    return first[1, 1] + second[1, 1]
}
"#,
    );
    assert_eq!(exit, 8);
}

/// Appendix B row 4: `@` binds tighter than `*` and `+`, so this is `(a @ b) * two`.
#[test]
fn the_operator_binds_tighter_than_multiplication() {
    let exit = run_program(
        "matmul_precedence.nr",
        r#"
func main() -> i32 {
    val a: Tensor<i32, [2, 2]> = [[1, 2], [3, 4]]
    val b: Tensor<i32, [2, 2]> = [[1, 0], [0, 1]]
    val two: Tensor<i32, [2, 2]> = [[2, 2], [2, 2]]
    val scaled = &a @ &b * &two
    return scaled[1, 1]
}
"#,
    );
    assert_eq!(exit, 8);
}

/// One body, specialized per instantiation, with the repeated `K` checked once.
#[test]
fn a_shape_generic_product_specializes_per_instantiation() {
    let exit = run_program(
        "matmul_generic.nr",
        r#"
func product<M, N, K>(a: &Tensor<i32, [M, K]>, b: &Tensor<i32, [K, N]>) -> Tensor<i32, [M, N]> {
    return a @ b
}

func main() -> i32 {
    val wide: Tensor<i32, [1, 3]> = [[1, 2, 3]]
    val tall: Tensor<i32, [3, 1]> = [[4], [5], [6]]
    val dot = product(&wide, &tall)
    val outer = product(&tall, &wide)
    return dot[0, 0] + outer[2, 2]
}
"#,
    );
    // [1, 2, 3] . [4, 5, 6] = 32, and the outer product's [2, 2] is 6 * 3 = 18.
    assert_eq!(exit, 50);
}

/// A chain releases the inner product's buffer: it is an owned operand of the outer one.
#[test]
fn a_chained_product_consumes_its_intermediate() {
    let exit = run_program(
        "matmul_chain.nr",
        r#"
func main() -> i32 {
    val a: Tensor<i32, [2, 2]> = [[1, 1], [0, 1]]
    val b: Tensor<i32, [2, 2]> = [[1, 0], [1, 1]]
    val c: Tensor<i32, [2, 2]> = [[2, 0], [0, 2]]
    val chained = &a @ &b @ &c
    return chained[0, 0]
}
"#,
    );
    // ([[1,1],[0,1]] @ [[1,0],[1,1]])[0, 0] = 2, doubled by the diagonal c.
    assert_eq!(exit, 4);
}
