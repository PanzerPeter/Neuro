// Einstein notation: `einsum("bij,bjk->bik", a, b)`
//
// One subscript per operand on the left of `->`, one letter per axis, and the result's
// axes on the right. A letter that appears in an input but not in the output is summed
// over; a letter that appears in the output takes its extent from wherever an input
// bound it. That one rule covers matrix products, traces, outer products, transposes,
// axis sums and batched contractions, which is why `einsum` replaces six method names
// with one call.
//
// The subscripts are a STRING LITERAL, never a value. The compiler reads them while it
// type-checks, matches each letter against the operands' shapes, and derives the result
// type from the letters after `->`. A subscript that disagrees with its operands — a
// letter bound to two different extents, an output letter no input binds — is a compile
// error naming the letter.
//
// An empty output subscript contracts everything away, so the call produces one number
// of the element type, exactly as a whole-tensor `.sum()` does.
//
// `einsum` READS its operands. It allocates its own result and leaves the buffers it
// contracted alone, so it works through a borrow.

// A borrowed weight matrix: `w` stays with the caller.
func project(w: &Tensor<f32, [2, 3]>, x: &Tensor<f32, [3]>) -> Tensor<f32, [2]> {
    einsum("ij,j->i", w, x)
}

func main() -> i32 {
    val a: Tensor<i32, [2, 3]> = [
        [1, 2, 3],
        [4, 5, 6]
    ]
    val b: Tensor<i32, [3, 2]> = [
        [1, 0],
        [0, 1],
        [1, 1]
    ]

    // The shared letter `j` appears in neither output axis, so it is contracted: this is
    // the matrix product, spelled out.
    val m: Tensor<i32, [2, 2]> = einsum("ij,jk->ik", a, b)
    println("product    = [{m[0, 0]}, {m[0, 1]}] [{m[1, 0]}, {m[1, 1]}]")

    // One operand, one letter, used twice: the subscript walks the diagonal and the
    // empty output sums it, which is the trace.
    val square: Tensor<i32, [3, 3]> = [
        [1, 2, 3],
        [4, 5, 6],
        [7, 8, 9]
    ]
    val trace: i32 = einsum("ii->", square)
    println("trace      = {trace}")

    // Nothing is contracted here, so every pair of positions survives into the result.
    val u: Tensor<i32, [2]> = [3, 4]
    val v: Tensor<i32, [3]> = [1, 2, 5]
    val outer: Tensor<i32, [2, 3]> = einsum("i,j->ij", u, v)
    println("outer      = [{outer[0, 0]}, {outer[0, 1]}, {outer[0, 2]}] [{outer[1, 0]}, {outer[1, 1]}, {outer[1, 2]}]")

    // Both letters survive, in the other order: a transpose without touching a buffer's
    // contents.
    val t: Tensor<i32, [3, 2]> = einsum("ij->ji", a)
    println("transposed = [{t[0, 0]}, {t[0, 1]}] [{t[1, 0]}, {t[1, 1]}] [{t[2, 0]}, {t[2, 1]}]")

    // Dropping a letter sums over it, which is what `.sum(axis:)` spells for one axis.
    val rows: Tensor<i32, [2]> = einsum("ij->i", a)
    val cols: Tensor<i32, [3]> = einsum("ij->j", a)
    println("row sums   = {rows[0]}, {rows[1]}")
    println("col sums   = {cols[0]}, {cols[1]}, {cols[2]}")

    // A letter shared by both operands AND the output batches the contraction: `b`
    // indexes all three at once instead of being summed.
    val batch: Tensor<i32, [2, 2, 3]> = [
        [[1, 2, 3], [4, 5, 6]],
        [[7, 8, 9], [1, 1, 1]]
    ]
    val kernels: Tensor<i32, [2, 3, 2]> = [
        [[1, 0], [0, 1], [1, 1]],
        [[2, 0], [0, 2], [1, 1]]
    ]
    val batched: Tensor<i32, [2, 2, 2]> = einsum("bij,bjk->bik", batch, kernels)
    println("batch 0    = [{batched[0, 0, 0]}, {batched[0, 0, 1]}] [{batched[0, 1, 0]}, {batched[0, 1, 1]}]")
    println("batch 1    = [{batched[1, 0, 0]}, {batched[1, 0, 1]}] [{batched[1, 1, 0]}, {batched[1, 1, 1]}]")

    // Two vectors, one shared letter, no output axes: the dot product is a scalar of the
    // element type, not a rank-0 tensor.
    val dot: i32 = einsum("i,i->", u, u)
    println("dot        = {dot}")

    // Through a borrow, in float, and the operands survive the call.
    val w: Tensor<f32, [2, 3]> = [
        [1.0, 0.0, 2.0],
        [0.0, 1.0, 0.5]
    ]
    val x: Tensor<f32, [3]> = [4.0, 6.0, 2.0]
    val y = project(&w, &x)
    println("projected  = {y[0]:.1}, {y[1]:.1}")
    println("weights    = {w[0, 0]:.1}, still readable")

    return m[0, 0] + m[1, 1] + dot
}
