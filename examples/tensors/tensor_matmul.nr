// Matrix multiplication: the `@` operator.
//
// `@` is the one tensor operator that is not element-wise. `a * b` multiplies the two
// buffers position for position; `a @ b` CONTRACTS an axis, so element `[i, j]` of the
// result is the dot product of row `i` of the left operand and column `j` of the right.
// That is the operation every linear layer in a network is written with, which is why it
// gets an operator of its own rather than a method.
//
// THE SHAPE RULE IS `[M, K] @ [K, N]` -> `[M, N]`. The two inner extents must agree, and
// they are what disappears from the result. Both operands are RANK 2: `@` neither
// broadcasts nor stretches, so there is no vector form and no scalar form. A shape
// mismatch is a compile error, not a run-time one.
//
// PRECEDENCE: `@` binds tighter than `*` and `+`, matching the convention on paper, so
// `a @ b + c` adds `c` to the product. It binds looser than `as`.
//
// Like every tensor operator it accepts owned or borrowed operands: `&w @ &x` reads a
// weight without moving it out of the binding that owns it.

// One projection, written once for every width in the program. `M`, `K` and `N` are
// compile-time values inferred from the operands' own shapes, and the repeated `K` is
// what makes a mismatched pair a compile error at the call site.
func project<M, N, K>(w: &Tensor<f32, [M, K]>, x: &Tensor<f32, [K, N]>) -> Tensor<f32, [M, N]> {
    return w @ x
}

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

    // [2, 3] @ [3, 2] contracts the 3 away and leaves a [2, 2]. Row 0 of `a` against
    // column 0 of `b` is 1*7 + 2*9 + 3*11 = 58.
    val c = &a @ &b
    println("[2, 3] @ [3, 2] = [[{c[0, 0]}, {c[0, 1]}], [{c[1, 0]}, {c[1, 1]}]]")

    // The identity leaves every element alone, which is the cheapest whole matrix to
    // check: a single wrong accumulator anywhere would change one of these.
    val weights: Tensor<f32, [2, 2]> = [
        [0.5, 1.5],
        [2.5, 3.5]
    ]
    val identity = Tensor::<f32, [2, 2]>::identity()
    val same = &weights @ &identity
    assert(same[0, 1] == weights[0, 1])
    assert(same[1, 0] == weights[1, 0])
    println("w @ I           leaves every element alone")

    // A borrowed operand is read, not consumed, so one weight matrix feeds two batches.
    val batch: Tensor<f32, [2, 3]> = [
        [1.0, 0.0, 2.0],
        [0.0, 1.0, 0.0]
    ]
    val first = project(&weights, &batch)
    val second = project(&weights, &batch)
    assert(first[1, 2] == second[1, 2])
    println("project<M, N, K> [2, 2] @ [2, 3] -> [2, 3], corner {first[1, 2]}")

    // `@` before `*`: the product is formed first, then scaled element-wise.
    val doubled: Tensor<f32, [2, 3]> = [
        [2.0, 2.0, 2.0],
        [2.0, 2.0, 2.0]
    ]
    val scaled = &weights @ &batch * &doubled
    assert(scaled[0, 0] == first[0, 0] * 2.0)
    println("w @ x * 2       binds as (w @ x) * 2, giving {scaled[0, 0]}")

    return c[1, 1] - 150
}
