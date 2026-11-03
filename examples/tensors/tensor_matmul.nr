// Matrix multiplication: the `@` operator. `@` contracts an axis, unlike the element-wise
// `*`: element `[i, j]` of `a @ b` is the dot product of row `i` of `a` and column `j` of
// `b`. Every linear layer is written with it. The shape rule is `[M, K] @ [K, N] -> [M,
// N]`. Both operands are rank 2, `@` does not broadcast, and a shape mismatch is a compile
// error. `@` shares the precedence of `*`, `/` and `%`, so `a @ b * c` groups left to
// right. It binds tighter than `+` and looser than `as`. Like every tensor operator it
// accepts owned or borrowed operands: `&w @ &x` reads a weight without moving it.

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

    // `@` before `*`: left to right, the product is formed first, then scaled element-wise.
    val doubled: Tensor<f32, [2, 3]> = [
        [2.0, 2.0, 2.0],
        [2.0, 2.0, 2.0]
    ]
    val scaled = &weights @ &batch * &doubled
    assert(scaled[0, 0] == first[0, 0] * 2.0)
    println("w @ x * 2       binds as (w @ x) * 2, giving {scaled[0, 0]}")

    // `*` before `@`: the same row, so the weight is halved first and then contracted.
    val halved = &weights * 0.5 @ &batch
    assert(halved[0, 0] == first[0, 0] * 0.5)
    println("w * 0.5 @ x     binds as (w * 0.5) @ x, giving {halved[0, 0]}")

    return c[1, 1] - 150
}
