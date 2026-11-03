// Tensor slicing and indexing: `t[i, j]`, `t[0, ..]` and `t[1..3, 2..5]`. Each axis gets a
// position, a range or `..`. A position drops its axis from the result, so naming every
// axis reads one element. A range or `..` keeps its axis at the new extent. A position may
// be a run-time integer, but range bounds must be constants, since the extent is part of
// the result's type. A slice is a fresh owned tensor holding a copy, not a view, so two
// tensors never share a buffer.

// A borrow reads a tensor without consuming it, so the caller keeps its own.

func trace(m: &Tensor<i32, [3, 3]>) -> i32 {
    mut total = 0
    for i in 0..3 {
        total = total + m[i, i]
    }
    return total
}

// A slice is an ordinary tensor value, so it passes to a function by value like any
// other: and moves, like any other.
func sum_row(row: Tensor<i32, [3]>) -> i32 {
    return row[0] + row[1] + row[2]
}

func main() -> i32 {
    val grid: Tensor<i32, [3, 3]> = [
        [1, 2, 3],
        [4, 5, 6],
        [7, 8, 9]
    ]

    // Every axis given a position: one element, typed as the element type.
    val centre = grid[1, 1]
    println("grid[1, 1]          = {centre}")

    // One axis kept whole: a row and a column are both rank-1 tensors.
    val row: Tensor<i32, [3]> = grid[2, ..]
    val column: Tensor<i32, [3]> = grid[.., 0]
    println("grid[2, ..][1]      = {row[1]}")
    println("grid[.., 0][2]      = {column[2]}")

    // Ranged slicing, exclusive and inclusive. Both axes survive, at the extent the
    // range names.
    val corner: Tensor<i32, [2, 2]> = grid[0..2, 0..2]
    val inclusive: Tensor<i32, [2, 2]> = grid[1..=2, 1..=2]
    println("grid[0..2, 0..2]    = [[{corner[0, 0]}, {corner[0, 1]}], [{corner[1, 0]}, {corner[1, 1]}]]")
    println("grid[1..=2, 1..=2]  = [[{inclusive[0, 0]}, {inclusive[0, 1]}], [{inclusive[1, 0]}, {inclusive[1, 1]}]]")

    // A rank-3 tensor: channels kept whole, the other two axes sliced.
    val cube: Tensor<i32, [2, 3, 4]> = [
        [[0, 1, 2, 3], [4, 5, 6, 7], [8, 9, 10, 11]],
        [[12, 13, 14, 15], [16, 17, 18, 19], [20, 21, 22, 23]]
    ]
    val patch: Tensor<i32, [2, 2, 2]> = cube[.., 1..3, 1..=2]
    println("cube[.., 1..3, 1..=2] corners = {patch[0, 0, 0]}, {patch[1, 1, 1]}")

    // A position may come from a run-time value: this one walks the diagonal.
    val diagonal = trace(&grid)
    println("trace(&grid)        = {diagonal}")

    // A slice is owned, so it may be sliced again and then moved away.
    val bottom = grid[2, ..]
    val total = sum_row(bottom)
    println("sum of the last row = {total}")

    return centre + diagonal + total
}
