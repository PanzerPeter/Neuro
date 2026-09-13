// Dynamic tensor shapes: `Tensor<f32, [?, 784]>`
//
// A `?` marks an axis whose extent is not known until run time. It is an ACCEPTING
// position: one function written against `[?, 4]` takes a `[2, 4]` and a `[7, 4]` alike,
// while the axes beside it stay checked exactly as before — `[2, 8]` is still a compile
// error, because only the axis written `?` opted out.
//
// Widening is what `?` is for, and it goes one way. A statically shaped tensor is
// accepted wherever a `?` is expected; the reverse is not, because a dynamic tensor's
// run-time shape could be anything and a static annotation would let the next reader
// index it at strides its buffer may not have.
//
// A `?`-shaped value is a tensor like any other at run time — a DLPack handle addressing
// its buffer — so it binds, moves across a call boundary, and is released at scope exit
// with no extent involved. What needs an extent does not work on one and says so: a
// constructor and a tensor literal have no size to allocate, `.clone()` no size to copy,
// an index no strides to compute, and the four shape casts no element count. Build such
// a tensor at a static shape and pass it where the `?` is expected.

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
