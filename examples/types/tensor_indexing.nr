// Tensor slicing and indexing: `t[i, j]`, `t[0, ..]`, `t[1..3, 2..5]`
//
// A tensor index gives one argument per axis, and each argument is a position, a
// range, or the whole axis `..`. An axis given a POSITION is dropped from the result,
// so naming every axis reads one element; an axis given a range or `..` survives at
// its new extent, so the result is a smaller tensor.
//
// A position may be any integer value, including one only known at run time. A range's
// bounds must be constants, because the extent they produce is part of the result's
// TYPE and a type cannot wait for a value.
//
// A slice is a fresh owned tensor holding a copy, not a view into the source: a tensor
// owns its buffer and frees it when it goes out of scope, so two tensors never share
// one buffer.

// A borrow reads a tensor without consuming it, so the caller keeps its own.
func trace(m: &Tensor<i32, [3, 3]>) -> i32 {
    mut total = 0
    for i in 0..3 {
        total = total + m[i, i]
    }
    return total
}

// A slice is an ordinary tensor value, so it passes to a function by value like any
// other — and moves, like any other.
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
