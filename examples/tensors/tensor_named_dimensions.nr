// Named tensor dimensions: `Tensor<f32, [batch: 32, embed: 768]>`
//
// An axis may carry a name before its extent. The name is part of the type and is
// checked wherever both sides supply one, so a signature reads as documentation and a
// transposed argument is a compile error rather than a wrong answer at run time.
//
// A name does NOT make a shape a different type. `Tensor<i32, [3, 4]>` and
// `Tensor<i32, [rows: 3, cols: 4]>` are interchangeable: two tensor types agree when
// their element types agree, their ranks agree, and each pair of extents agrees, with
// names compared only at a position where BOTH sides name the axis. What is rejected
// is `[rows: 3, cols: 4]` against `[cols: 3, rows: 4]` — the same extents, the wrong
// way round.
//
// Names live in the tensor type's own namespace, not in the surrounding scope: the
// `rows` below is an axis name, and a local variable called `rows` would neither
// shadow it nor collide with it. The extent is still the only thing a generic can
// bind, so `[batch: N]` names the axis `batch` and leaves `N` as the shape parameter.

// The shape is the signature's documentation: every reader knows which axis is which.
func total_pixels(image: &Tensor<i32, [channels: 3, height: 2, width: 4]>) -> i32 {
    mut total = 0
    for c in 0..3 {
        for h in 0..2 {
            for w in 0..4 {
                total = total + image[c, h, w]
            }
        }
    }
    return total
}

// A named axis over a shape parameter: `width` names the axis, `W` is the extent that
// each call infers.
func row_width<W>(row: &Tensor<i32, [width: W]>) -> i32 {
    return W as i32
}

// An unnamed shape accepts a named argument, which is what keeps a named tensor usable
// by every function written before the names existed.
func first_element(t: &Tensor<i32, [2, 4]>) -> i32 {
    return t[0, 0]
}

func main() -> i32 {
    // The annotation names the axes; the literal is checked against the extents.
    val plane: Tensor<i32, [height: 2, width: 4]> = [
        [1, 2, 3, 4],
        [5, 6, 7, 8]
    ]

    // An unnamed shape and a named one with the same extents are one type.
    println("first_element(&plane)  = {first_element(&plane)}")

    // Indexing drops the axes given a position and keeps the names of the rest, so the
    // row below is a `[width: 4]` and answers to that name.
    val row: Tensor<i32, [width: 4]> = plane[1, ..]
    println("row[0]                 = {row[0]}")
    println("row_width(&row)        = {row_width(&row)}")

    // A constructor takes its shape from the type written at the call, names included.
    val eye: Tensor<i32, [row: 3, col: 3]> = Tensor::<i32, [row: 3, col: 3]>::identity()
    println("eye[2, 2]              = {eye[2, 2]}")

    val image: Tensor<i32, [channels: 3, height: 2, width: 4]> = Tensor::<i32, [3, 2, 4]>::ones()
    println("total_pixels(&image)   = {total_pixels(&image)}")

    return total_pixels(&image) + row_width(&row) + row[0]
}
