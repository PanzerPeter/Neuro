// By-value tensor operators: `a + b`, `&a + &b` and scalar broadcasting. A by-value
// operator allocates a fresh tensor from two operands. The in-place `+=` family writes into
// the buffer the target owns instead (`w -= g`), while `w = w - g` builds a second one. An
// owned operand is moved; a borrowed one (`&a`) is only read. Shapes broadcast from the
// trailing axis. An axis of extent 1 stretches across the other operand, and a lower-rank
// operand supplies the innermost axes. So `[2, 3]` combines with `[3]`, with `[2, 1]` and
// with a scalar, but not with `[2]`. A scalar takes the tensor's element type, so `2.0` is
// an `f32` literal in an `f32` tensor.

// A borrowed pair: neither operand is consumed, so the caller keeps both.

func combine(a: &Tensor<i32, [2, 3]>, b: &Tensor<i32, [2, 3]>) -> Tensor<i32, [2, 3]> {
    return a * b
}

func main() -> i32 {
    val left: Tensor<i32, [2, 3]> = [
        [1, 2, 3],
        [4, 5, 6]
    ]
    val right: Tensor<i32, [2, 3]> = [
        [10, 10, 10],
        [20, 20, 20]
    ]

    // Equal shapes: element for element.
    val sum = &left + &right
    println("sum[0, 0] = {sum[0, 0]}, sum[1, 2] = {sum[1, 2]}")

    // Both operands were borrowed above, so both are still here.
    val product = combine(&left, &right)
    println("product[1, 1] = {product[1, 1]}")

    // A rank-1 row aligns at the trailing axis and repeats down the rows.
    val row: Tensor<i32, [3]> = [100, 200, 300]
    val widened = &left + &row
    println("row broadcast: {widened[0, 2]} and {widened[1, 2]}")

    // A size-1 axis is stretched across the other operand's extent.
    val column: Tensor<i32, [2, 1]> = [[7], [9]]
    val stretched = &left + &column
    println("column broadcast: {stretched[0, 0]} and {stretched[1, 0]}")

    // Both operands stretch at once: a row against a column fills the whole grid.
    val across: Tensor<i32, [1, 3]> = [[1, 2, 3]]
    val down: Tensor<i32, [2, 1]> = [[10], [20]]
    val grid = &across * &down
    println("outer grid: {grid[0, 0]}, {grid[1, 2]}")

    // A scalar on either side, and the compound form of the same rule.
    val scaled = &left * 3
    val offset = 100 + left
    println("scalar broadcast: {scaled[1, 2]} and {offset[0, 0]}")

    mut weights: Tensor<f32, [2, 2]> = [
        [1.0, 2.0],
        [3.0, 4.0]
    ]
    val decay: Tensor<f32, [2]> = [0.5, 0.5]
    // `*=` and `+=` take the by-value operators' broadcast rules, and write the result
    // back into the buffer `weights` already owns: no allocation, same handle.
    weights *= 2.0
    weights -= &decay
    println("in place: {weights[0, 0]} and {weights[1, 1]}")

    // A rank-0 tensor has one element and no axis to broadcast along.
    val a = Tensor::<i32, []>::scalar(20)
    val b = Tensor::<i32, []>::scalar(22)
    val total = a + b
    println("rank 0: {total.item()}")

    return sum[1, 2] + grid[0, 0]
}
