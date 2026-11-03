// Dynamic tensor shapes: `Tensor<f32, [?, 784]>`. A `?` marks an axis whose extent is known
// only at run time. It accepts any extent there, so a function written against `[?, 4]`
// takes `[2, 4]` and `[7, 4]` alike, while the other axes stay checked: `[2, 8]` is still a
// compile error. Widening goes one way. A static tensor is accepted where a `?` is
// expected, but not the reverse, because a dynamic tensor's shape could be anything. A `?`
// tensor binds, moves and is released like any other. Operations that need an extent (a
// constructor, a tensor literal, `.clone()`, an index, the shape casts) reject it, so build
// it at a static shape and pass it where the `?` is expected.

// One signature, every batch size. The second axis is fixed, so a row of the wrong
// width is still rejected at compile time.

func embed_width(batch: &Tensor<i32, [?, 4]>) -> i32 {
    return 4
}

// A named axis may be dynamic too: the name documents the axis, the `?` says its extent
// arrives later, and the two are independent.
func batch_embed(batch: &Tensor<i32, [batch: ?, embed: 3]>) -> i32 {
    return 3
}

// Widening happens at the return: the body hands back a fully shaped tensor, and the
// signature forgets the first extent for every caller.
func as_batch(rows: Tensor<i32, [2, 4]>) -> Tensor<i32, [?, 4]> {
    return rows
}

func main() -> i32 {
    val pair: Tensor<i32, [2, 4]> = [
        [1, 2, 3, 4],
        [5, 6, 7, 8]
    ]
    val seven = Tensor::<i32, [7, 4]>::ones()

    // Two different extents at the same `?` axis, one function.
    println("embed_width(&pair)     = {embed_width(&pair)}")
    println("embed_width(&seven)    = {embed_width(&seven)}")

    val rows: Tensor<i32, [batch: 3, embed: 3]> = [
        [1, 0, 0],
        [0, 1, 0],
        [0, 0, 1]
    ]
    println("batch_embed(&rows)     = {batch_embed(&rows)}")

    // The statically shaped tensor still reads its own elements; the dynamic view of it
    // is what crosses the boundary.
    val first = pair[0, 3]
    println("pair[0, 3]             = {first}")

    // `pair` MOVES into the widened tensor, which owns the same buffer and frees it here
    // at the end of `main`.
    val batch = as_batch(pair)
    println("a [2, 4] widened to [?, 4]")

    return embed_width(&seven) * batch_embed(&rows) + first + 26
}
