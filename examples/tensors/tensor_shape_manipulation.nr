// Tensor shape manipulation: `.t()`, `.reshape(...)`, `.permute(...)` and `.flatten(...)`.
// All four return a tensor of a different shape and consume the receiver, since a tensor
// owns its buffer. Write `t.clone().t()` to keep the original. The result's shape is
// computed at compile time: `.reshape` cannot change the element count, `.t()` rejects any
// rank but 2, and `.permute` lists the names the shape does declare when given an unknown
// one. `.permute` and `.flatten` take dimension names as well as positions, resolved
// against the receiver's own shape, never the surrounding scope.

// The names make the axis order part of the signature, and `.permute` reads as the
// rearrangement it performs rather than as three anonymous numbers.

func to_channels_last(
    image: Tensor<i32, [channels: 2, height: 2, width: 3]>
) -> Tensor<i32, [height: 2, width: 3, channels: 2]> {
    return image.permute([height, width, channels])
}

func main() -> i32 {
    val m: Tensor<i32, [2, 3]> = [
        [1, 2, 3],
        [4, 5, 6]
    ]

    // `.t()` moves the elements, not just the labels: row 0 of the transpose is the
    // first column of `m`.
    val t = m.clone().t()
    println("m.t()[0, 1]           = {t[0, 1]}")
    println("m.t()[2, 0]           = {t[2, 0]}")

    // `.reshape` keeps row-major order, and `-1` takes whatever extent the others
    // leave over. A reshape allocates nothing: the buffer is re-described in place.
    val flat: Tensor<i32, [6]> = m.clone().reshape([-1])
    println("reshape([-1])[3]      = {flat[3]}")

    val rows = m.reshape([3, -1])
    println("reshape([3, -1])[2,1] = {rows[2, 1]}")

    // A local named `height` neither shadows the axis nor is shadowed by it.
    val height = "a local, not an axis"
    val image: Tensor<i32, [channels: 2, height: 2, width: 3]> = [
        [[1, 2, 3], [4, 5, 6]],
        [[7, 8, 9], [10, 11, 12]]
    ]
    val hwc = to_channels_last(image)
    println("{height}")
    println("hwc[0, 0, 1]          = {hwc[0, 0, 1]}")
    println("hwc[1, 2, 1]          = {hwc[1, 2, 1]}")

    // `.flatten(dims: [...])` merges an adjacent run of axes and leaves the rest alone;
    // a bare `.flatten()` merges them all into one.
    val batch: Tensor<i32, [batch: 2, seq_len: 2, embed: 3]> = [
        [[1, 1, 1], [2, 2, 2]],
        [[3, 3, 3], [4, 4, 4]]
    ]
    val tokens = batch.flatten(dims: [seq_len, embed])
    println("flatten(dims:)[1, 5]  = {tokens[1, 5]}")

    val square: Tensor<i32, [2, 2]> = [
        [1, 2],
        [3, 4]
    ]
    val all = square.flatten()
    println("flatten()[3]          = {all[3]}")

    return t[0, 1] * 10 + all[3]
}
