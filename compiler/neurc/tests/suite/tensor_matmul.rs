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

/// A product of fractional `f32`s whose sums depend on their order, against the naive
/// nest written out: each element starts at zero and adds its products one at a time, in
/// contracted order. 37 rows and 45 columns leave a remainder on both axes of the host's
/// register block, so the peeled loops are checked as well as the main one.
const ORDERED_PRODUCT: &str = r#"
func main() -> i32 {
    mut a: Tensor<f32, [37, 19]> = Tensor::zeros()
    mut b: Tensor<f32, [19, 45]> = Tensor::zeros()
    mut w: Tensor<f64, [9, 5]> = Tensor::zeros()
    mut x: Tensor<f64, [5, 33]> = Tensor::zeros()
    for i in 0..37 {
        for k in 0..19 {
            a[i, k] = ((i * 7 + k * 13) % 23) as f32 * 0.37f32 - 3.1f32
        }
    }
    for k in 0..19 {
        for j in 0..45 {
            b[k, j] = ((k * 5 + j * 3) % 29) as f32 * 0.013f32 + 0.7f32
        }
    }
    for i in 0..9 {
        for k in 0..5 {
            w[i, k] = ((i * 3 + k) % 7) as f64 * 0.1 - 0.35
        }
    }
    for k in 0..5 {
        for j in 0..33 {
            x[k, j] = ((k * 11 + j) % 13) as f64 * 0.07 + 0.003
        }
    }
    val c = &a @ &b
    val e = einsum("ij,jk->ik", &a, &b)
    val y = &w @ &x
    mut wrong = 0
    for i in 0..37 {
        for j in 0..45 {
            mut s = 0.0f32
            for k in 0..19 {
                s = s + a[i, k] * b[k, j]
            }
            if c[i, j] != s { wrong += 1 }
            if e[i, j] != s { wrong += 1 }
        }
    }
    for i in 0..9 {
        for j in 0..33 {
            mut s = 0.0
            for k in 0..5 {
                s = s + w[i, k] * x[k, j]
            }
            if y[i, j] != s { wrong += 1 }
        }
    }
    println("{c[36, 44]} {y[8, 32]}")
    return wrong
}
"#;

#[test]
fn a_blocked_product_adds_each_element_in_contracted_order() {
    assert_eq!(run_program("matmul_ordered.nr", ORDERED_PRODUCT), 0);
}

/// An integer product is blocked only where it wraps: on the debug tier its overflow check
/// still stops the program at the `@`.
#[test]
fn an_integer_product_wraps_on_release_and_traps_on_debug() {
    let test = CompileTest::new();
    let source = test.write_source(
        "matmul_wrap.nr",
        r#"
func main() -> i32 {
    mut a: Tensor<i32, [8, 20]> = Tensor::zeros()
    mut b: Tensor<i32, [20, 24]> = Tensor::zeros()
    for i in 0..8 {
        for k in 0..20 {
            a[i, k] = (i * 20 + k) * 9973 + 1000003
        }
    }
    for k in 0..20 {
        for j in 0..24 {
            b[k, j] = (k * 24 + j) * 7919 - 500009
        }
    }
    val c = &a @ &b
    mut wrong = 0
    for i in 0..8 {
        for j in 0..24 {
            mut s = 0
            for k in 0..20 {
                s = s.wrapping_add(a[i, k].wrapping_mul(b[k, j]))
            }
            if c[i, j] != s { wrong += 1 }
        }
    }
    return wrong
}
"#,
    );
    for (level, panic) in [("0", true), ("2", false)] {
        let exe = source.with_extension(format!("o{level}"));
        let built = std::process::Command::new(env!("CARGO_BIN_EXE_neurc"))
            .args(["compile", "-O", level, "-o"])
            .arg(&exe)
            .arg(&source)
            .output()
            .expect("neurc runs");
        assert!(built.status.success(), "{built:?}");
        let output = std::process::Command::new(&exe)
            .output()
            .expect("the program runs");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            stderr.starts_with("panic: integer overflow at "),
            panic,
            "-O{level}: {stderr}"
        );
        if !panic {
            assert_eq!(output.status.code(), Some(0), "-O{level}: {stderr}");
        }
    }
}
