// Tensor sorting and selection: `.sort()`, `.argsort()`, `.topk()`
//
// All three order one axis of a tensor and differ only in what they hand back: the
// elements in that order, the receiver positions that produce that order, or the leading
// `k` of both. With no `axis:` they work along the last axis, which is the row of a
// matrix; `axis:` may be a position, a dimension NAME the receiver's type declares, or a
// negative index counting from the end.
//
// They are NATIVE to the element dtype, so `f32` and `f64` sort without wrapping every
// element in an ordered-float type first. The comparator is IEEE-754-ordered with one
// rule: `NaN` sorts to the END, whether the order is ascending or descending. Real
// workloads treat `NaN` as "invalid", so pushing it to the back leaves the best
// candidates at the front where top-k expects them.
//
// Equal elements keep the order they were written in, which is the only thing that makes
// an argsort of a tensor with ties mean anything.
//
// Like a reduction, a selection READS its receiver: it allocates its own result and
// leaves the buffer it ordered alone, so it works through a borrow.

// A borrowed receiver: `scores` stays with the caller.
func best(scores: &Tensor<i32, [5]>) -> i32 {
    val ranked: Tensor<i32, [5]> = scores.sort(descending: true)
    ranked[0]
}

func main() -> i32 {
    val scores: Tensor<i32, [5]> = [5, 3, 9, 1, 7]

    val up: Tensor<i32, [5]> = scores.sort()
    val down: Tensor<i32, [5]> = scores.sort(descending: true)
    println("ascending  = {up[0]}, {up[1]}, {up[2]}, {up[3]}, {up[4]}")
    println("descending = {down[0]}, {down[1]}, {down[2]}, {down[3]}, {down[4]}")

    // `.argsort()` answers positions, not values, so an entry reads back into the tensor
    // it came from.
    val order: Tensor<i32, [5]> = scores.argsort()
    val lowest = order[0] as u64
    println("order      = {order[0]}, {order[1]}, {order[2]}, {order[3]}, {order[4]}")
    println("smallest   = {scores[lowest]} at position {order[0]}")

    // `.topk` selects the k GREATEST, and pairs them with where they came from.
    val (top, at) = scores.topk(k: 3)
    println("top three  = {top[0]}, {top[1]}, {top[2]} at {at[0]}, {at[1]}, {at[2]}")

    // A named axis orders by name. `height` runs down the columns.
    val grid: Tensor<i32, [height: 2, width: 3]> = [
        [7, 2, 5],
        [1, 9, 4]
    ]
    val rows: Tensor<i32, [height: 2, width: 3]> = grid.sort()
    val cols: Tensor<i32, [height: 2, width: 3]> = grid.sort(axis: height)
    println("rows       = [{rows[0, 0]}, {rows[0, 1]}, {rows[0, 2]}] [{rows[1, 0]}, {rows[1, 1]}, {rows[1, 2]}]")
    println("columns    = [{cols[0, 0]}, {cols[0, 1]}, {cols[0, 2]}] [{cols[1, 0]}, {cols[1, 1]}, {cols[1, 2]}]")

    // Two per row, and the selected axis is two long rather than three.
    val (pair, pair_at) = grid.topk(k: 2, axis: width)
    println("top two    = [{pair[0, 0]}, {pair[0, 1]}] [{pair[1, 0]}, {pair[1, 1]}] at [{pair_at[0, 0]}, {pair_at[0, 1]}] [{pair_at[1, 0]}, {pair_at[1, 1]}]")

    // Floats sort natively, and a `NaN` lands at the end of both orders.
    val nan = 0.0 / 0.0
    val readings: Tensor<f64, [4]> = [3.5, nan, 1.5, 2.5]
    val quiet: Tensor<f64, [4]> = readings.sort()
    val loud: Tensor<f64, [4]> = readings.sort(descending: true)
    println("quiet      = {quiet[0]:.1}, {quiet[1]:.1}, {quiet[2]:.1}, nan={quiet[3].is_nan()}")
    println("loud       = {loud[0]:.1}, {loud[1]:.1}, {loud[2]:.1}, nan={loud[3].is_nan()}")

    // The receiver survives: read through a borrow, then read again here.
    println("best={best(&scores)} total={scores.sum()}")

    return top[0] + top[1] + top[2]
}
