// Matrix multiplication (Phase 2C): the `@` operator, end to end through
// `neurc compile` and the linked binary.
//
// `@` is the one tensor operator that is not element-wise. It contracts the operands'
// inner axis (`[M, K] @ [K, N]` gives `[M, N]`), so unlike `+` it accepts no scalar and
// only the axes in front of the two matrix axes broadcast, as batch axes. It shares
// `*`'s precedence row (Appendix B row 4), so a mix of the two groups left to right:
// `a @ b * c` scales the product, `a * b @ c` the operand.

use crate::compile_harness::{CompileTest, assert_check_error, run, stdout_of};

#[test]
fn a_matrix_product_computes_every_element() {
    // [[1, 2, 3], [4, 5, 6]] @ [[7, 8], [9, 10], [11, 12]] = [[58, 64], [139, 154]]
    let exit = run(
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
    let exit = run(
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

/// A borrowed operand is read rather than consumed, so one weight feeds two products:
/// the reason every tensor operator is defined on `&T` as well.
#[test]
fn a_borrowed_operand_survives_the_product() {
    let exit = run(
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

/// Appendix B row 4: `@` and `*` share a row and group left to right, so this is
/// `(a @ b) * two`.
#[test]
fn a_product_followed_by_multiplication_scales_the_product() {
    let exit = run(
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

/// The other half of the shared row: `(a * mask) @ b`, which gives 12. Read as
/// `a * (mask @ b)` it would be `a * b` here and give 9.
#[test]
fn multiplication_followed_by_a_product_scales_the_operand() {
    let exit = run(
        "matmul_after_multiply.nr",
        r#"
func main() -> i32 {
    val a: Tensor<i32, [2, 2]> = [[1, 2], [3, 4]]
    val mask: Tensor<i32, [2, 2]> = [[1, 0], [0, 1]]
    val b: Tensor<i32, [2, 2]> = [[1, 2], [3, 4]]
    val product = &a * &mask @ &b
    return product[1, 0]
}
"#,
    );
    assert_eq!(exit, 12);
}

/// Grouping left to right puts the scalar on `@`, which takes no scalar operand. Read as
/// `a @ (2 * b)` it would type-check.
#[test]
fn a_scalar_left_of_a_later_multiplication_is_a_matmul_operand() {
    let errors = CompileTest::new()
        .check(
            "matmul_scalar_operand.nr",
            r#"
func main() -> i32 {
    val a: Tensor<i32, [2, 2]> = [[1, 2], [3, 4]]
    val b: Tensor<i32, [2, 2]> = [[1, 0], [0, 1]]
    val c = &a @ 2 * &b
    return c[0, 0]
}
"#,
        )
        .expect_err("`a @ 2 * b` contracts against a scalar");
    assert!(
        errors.contains("the operands of `@` do not multiply: &Tensor<i32, [2, 2]> against i32"),
        "{errors}"
    );
}

/// One body, specialized per instantiation, with the repeated `K` checked once.
#[test]
fn a_shape_generic_product_specializes_per_instantiation() {
    let exit = run(
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
    let exit = run(
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
    assert_eq!(run("matmul_ordered.nr", ORDERED_PRODUCT), 0);
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

/// Two batches of `[2, 3] @ [3, 2]`, each worked by hand: batch 0 is
/// `[[4, 5], [10, 11]]` and batch 1 `[[2, 3], [4, 5]]`, 44 in all. A rank-2 left operand
/// repeats across the batch, as does one whose batch axis is 1; its second batch is
/// `[[28, 34], [64, 79]]`. A rank-2 right operand repeats the same way.
#[test]
fn a_batched_product_multiplies_each_batch_element() {
    let source = r#"
func main() -> i32 {
    val a: Tensor<i32, [2, 2, 3]> = [[[1, 2, 3], [4, 5, 6]], [[1, 0, 0], [0, 1, 0]]]
    val b: Tensor<i32, [2, 3, 2]> = [[[1, 0], [0, 1], [1, 1]], [[2, 3], [4, 5], [6, 7]]]
    val c = &a @ &b
    println("{c.sum()} {c[0, 1, 0]} {c[1, 1, 1]}")
    val m: Tensor<i32, [2, 3]> = [[1, 2, 3], [4, 5, 6]]
    val d = &m @ &b
    println("{d.sum()} {d[1, 0, 0]} {d[1, 1, 1]}")
    val one: Tensor<i32, [1, 2, 3]> = [[[1, 2, 3], [4, 5, 6]]]
    val e = &one @ &b
    println("{e.sum()} {e[1, 0, 1]}")
    val f = &b @ &m
    println("{f.sum()} {f[1, 2, 2]}")
    return 0
}
"#;
    assert_eq!(
        stdout_of("batched.nr", source),
        "44 10 5\n235 28 79\n235 34\n339 60\n"
    );
}

/// The batched product is the same sum in every spelling: the operator as a tail, bound
/// and returned, through a shape-generic splice, inside a closure and a `match` arm, as a
/// call argument, and as `einsum` with batch letters and with `...`.
#[test]
fn a_batched_product_agrees_in_every_form() {
    let source = r#"
func tail(a: &Tensor<i32, [2, 2, 3]>, b: &Tensor<i32, [2, 3, 2]>) -> i32 {
    (a @ b).sum()
}

func bound(a: &Tensor<i32, [2, 2, 3]>, b: &Tensor<i32, [2, 3, 2]>) -> i32 {
    val product = a @ b
    return product.sum()
}

func spliced<S, M, K, N>(a: &Tensor<i32, [..S, M, K]>, b: &Tensor<i32, [..S, K, N]>) -> Tensor<i32, [..S, M, N]> {
    a @ b
}

func total(t: Tensor<i32, [2, 2, 2]>) -> i32 {
    t.sum()
}

func main() -> i32 {
    val a: Tensor<i32, [2, 2, 3]> = [[[1, 2, 3], [4, 5, 6]], [[1, 0, 0], [0, 1, 0]]]
    val b: Tensor<i32, [2, 3, 2]> = [[[1, 0], [0, 1], [1, 1]], [[2, 3], [4, 5], [6, 7]]]
    val closure = |x: &Tensor<i32, [2, 2, 3]>, y: &Tensor<i32, [2, 3, 2]>| -> i32 { (x @ y).sum() }
    val arm = match a.sum() > 0 {
        true => (&a @ &b).sum(),
        false => 0,
    }
    val lettered = einsum("bij,bjk->bik", &a, &b).sum()
    val ellipsis = einsum("...ij,...jk->...ik", &a, &b).sum()
    println("{tail(&a, &b)} {bound(&a, &b)} {spliced(&a, &b).sum()} {closure(&a, &b)} {arm} {total(&a @ &b)} {lettered} {ellipsis}")
    return 0
}
"#;
    assert_eq!(
        stdout_of("batched_forms.nr", source),
        "44 44 44 44 44 44 44 44\n"
    );
}

/// `[..S, M, K] @ [..S, K, N]` is one template for every batch rank, `S = []` included.
#[test]
fn a_spliced_batch_serves_every_rank() {
    let source = r#"
func bmm<S, M, K, N>(a: &Tensor<f32, [..S, M, K]>, b: &Tensor<f32, [..S, K, N]>) -> Tensor<f32, [..S, M, N]> {
    a @ b
}

func main() -> i32 {
    val a = Tensor::<f32, [2, 3, 5, 7]>::ones()
    val b = Tensor::<f32, [2, 3, 7, 70]>::ones()
    val c = bmm(&a, &b)
    val a2 = Tensor::<f32, [5, 7]>::ones()
    val b2 = Tensor::<f32, [7, 70]>::ones()
    println("{c.sum()} {c[1, 2, 4, 69]} {bmm(&a2, &b2).sum()}")
    return 0
}
"#;
    assert_eq!(stdout_of("spliced.nr", source), "14700.0 7.0 2450.0\n");
}

/// A batched float product is blocked and peeled like a plain one, and each element still
/// adds its products in contracted order, so it matches a scalar loop bit for bit, a
/// stretched batch axis included.
#[test]
fn a_batched_product_adds_each_element_in_contracted_order() {
    let source = r#"
func main() -> i32 {
    mut a = Tensor::<f32, [3, 9, 70]>::zeros()
    mut w = Tensor::<f32, [1, 70, 33]>::zeros()
    for s in 0..3 {
        for i in 0..9 {
            for k in 0..70 {
                a[s, i, k] = ((s * 11 + i * 7 + k * 13) % 23) as f32 * 0.37f32 - 3.1f32
            }
        }
    }
    for k in 0..70 {
        for j in 0..33 {
            w[0, k, j] = ((k * 5 + j * 3) % 29) as f32 * 0.013f32 + 0.7f32
        }
    }
    val c = &a @ &w
    mut wrong = 0
    for s in 0..3 {
        for i in 0..9 {
            for j in 0..33 {
                mut sum = 0.0f32
                for k in 0..70 {
                    sum = sum + a[s, i, k] * w[0, k, j]
                }
                if c[s, i, j] != sum { wrong += 1 }
            }
        }
    }
    return wrong
}
"#;
    assert_eq!(run("batched_ordered.nr", source), 0);
}

/// The gradient of a batched product sums over the batch an operand was stretched across.
/// By hand: `x`'s column sums over batch and rows are 5, 5 and 7, so every `dw` column is
/// `[5, 5, 7]`, 34 in all; `w`'s row sums are 3, 7 and 11, and the loss is
/// `5 * 3 + 5 * 7 + 7 * 11`.
#[test]
fn a_batched_product_differentiates_over_a_stretched_batch() {
    let source = r#"
@grad(wrt: [w, x])
func loss(w: &mut Tensor<f32, [1, 3, 2]>, x: &mut Tensor<f32, [2, 4, 3]>) -> Tensor<f32, []> {
    Tensor::scalar((x @ w).sum())
}

@grad(wrt: [w])
func squared(w: &mut Tensor<f32, [3, 2]>, x: &Tensor<f32, [2, 4, 3]>) -> Tensor<f32, []> {
    val y = x @ w
    Tensor::scalar((&y * &y).sum())
}

func main() -> i32 {
    mut w: Tensor<f32, [1, 3, 2]> = [[[1.0, 2.0], [3.0, 4.0], [5.0, 6.0]]]
    mut x: Tensor<f32, [2, 4, 3]> = [
        [[1.0, 2.0, 3.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
        [[2.0, 0.0, 0.0], [0.0, 0.0, 2.0], [1.0, 1.0, 1.0], [0.0, 1.0, 0.0]]
    ]
    val l = loss(&mut w, &mut x)
    println("{l.item()}")
    l.backward()
    println("{w.grad().sum()} {w.grad()[0, 2, 1]} {x.grad().sum()} {x.grad()[1, 3, 2]}")
    mut v: Tensor<f32, [3, 2]> = [[1.0, 0.0], [0.0, 1.0], [1.0, 1.0]]
    val m = squared(&mut v, &x)
    m.backward()
    println("{m.item()} {v.grad().sum()} {v.grad()[2, 0]}")
    return 0
}
"#;
    assert_eq!(
        stdout_of("batched_grad.nr", source),
        "127.0\n34.0 7.0 168.0 11.0\n66.0 166.0 38.0\n"
    );
}

#[test]
fn batch_axes_that_do_not_broadcast_are_rejected() {
    assert_check_error(
        "batch_mismatch.nr",
        "func main() -> i32 {\n    val a = Tensor::<f32, [2, 2, 3]>::ones()\n    \
         val b = Tensor::<f32, [3, 3, 2]>::ones()\n    val c = &a @ &b\n    0\n}\n",
        "and broadcasts any leading batch axes",
    );
    assert_check_error(
        "splice_mismatch.nr",
        "func f<S>(a: &Tensor<f32, [..S, 2, 3]>, b: &Tensor<f32, [4, 3, 2]>) -> f32 { (a @ b).sum() }\n\
         func main() -> i32 { 0 }\n",
        "do not multiply",
    );
}

/// A splice may stand for batch axes but not for a matrix axis, and a vector is never
/// promoted: the diagnostic names the reshape that makes it a matrix.
#[test]
fn a_matrix_axis_must_be_written() {
    assert_check_error(
        "splice_matrix.nr",
        "func f<S>(a: &Tensor<f32, [2, ..S]>, b: &Tensor<f32, [3, 2]>) -> f32 { (a @ b).sum() }\n\
         func main() -> i32 { 0 }\n",
        "`@` needs the rank",
    );
    assert_check_error(
        "vector.nr",
        "func main() -> i32 {\n    val m = Tensor::<f32, [2, 3]>::ones()\n    \
         val v = Tensor::<f32, [3]>::ones()\n    val c = &m @ &v\n    0\n}\n",
        "m @ v.reshape([-1, 1])",
    );
}
