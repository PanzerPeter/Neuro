// Tensor reductions: `.sum()`, `.mean()`, `.max()` and `.min()`. With no argument a
// reduction folds every element into one scalar of the element type. With `axis:` it folds
// that axis alone and drops it: a `[2, 3]` summed along axis 1 is a `[2]`. The axis can be
// a position, a dimension name the type declares, or a negative index counting from the
// end. A reduction reads its receiver and allocates a smaller tensor, so it accepts a
// borrow. Reducing a rank-1 tensor leaves a rank-0 `Tensor<T, []>`, which `.item()` reads
// as a plain `T`. `.mean()` is float-only: an integer mean would need a rounding rule the
// language does not give.

// A borrowed receiver: `scores` stays with the caller.

func spread(scores: &Tensor<i32, [4]>) -> i32 {
    scores.max() - scores.min()
}

func main() -> i32 {
    val grid: Tensor<i32, [2, 3]> = [
        [1, 2, 3],
        [4, 5, 6]
    ]

    // The whole buffer, folded three ways.
    println("sum={grid.sum()} max={grid.max()} min={grid.min()}")

    // Along an axis: one number per surviving position.
    val row_totals: Tensor<i32, [2]> = grid.sum(axis: 1)
    val col_totals: Tensor<i32, [3]> = grid.sum(axis: 0)
    println("row totals = {row_totals[0]}, {row_totals[1]}")
    println("col totals = {col_totals[0]}, {col_totals[1]}, {col_totals[2]}")

    // A named axis reduces by name, and the surviving axis keeps its own name.
    val frame: Tensor<f64, [height: 2, width: 3]> = [
        [0.0, 3.0, 6.0],
        [1.0, 4.0, 9.0]
    ]
    val column_means: Tensor<f64, [width: 3]> = frame.mean(axis: height)
    println("column means = {column_means[0]:.1}, {column_means[1]:.1}, {column_means[2]:.1}")

    // `-1` is the last axis, so this is the brightest value in each row.
    val row_peaks: Tensor<f64, [height: 2]> = frame.max(axis: -1)
    println("row peaks    = {row_peaks[0]:.1}, {row_peaks[1]:.1}")
    println("frame mean   = {frame.mean():.2}")

    // The receiver survives: `scores` is read through a borrow, then read again here.
    val scores: Tensor<i32, [4]> = [4, 9, 2, 7]
    println("spread={spread(&scores)} total={scores.sum()}")

    // Folding the only axis leaves rank 0; `.item()` reads the element without moving it.
    val folded: Tensor<i32, []> = scores.sum(axis: 0)
    println("folded = {folded.item()}, peak = {scores.max(axis: 0).item()}")

    return grid.sum()
}
