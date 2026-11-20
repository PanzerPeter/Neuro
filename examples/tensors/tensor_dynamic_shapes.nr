// Dynamic tensor shapes: `Tensor<f32, [?B, 784]>`. A `?` marks an axis whose extent is known
// only at run time, so one compiled body serves every batch size. `?B` names the extent:
// every `?B` in a signature or body is the same value, so two tensors carrying it combine
// element-wise and under `@` with no check, and `B` itself reads as a `u64`. A bare `?` is
// equal to nothing, so it combines with nothing until `.as_shape` narrows it.
//
// A name gets its value where a tensor first meets it: a call's arguments (a later argument
// that disagrees panics before the body runs), a `val` annotation, a constructor's `dims:`,
// a struct literal, or a fresh name in `.as_shape`.

// The batch extent arrives with `x`; the weights are static. The result keeps `B`.
func scores(x: &Tensor<f32, [?B, 3]>, w: &Tensor<f32, [3, 2]>) -> Tensor<f32, [?B, 2]> {
    x @ w
}

// `B` is a value inside the scope, like an extent parameter.
func mean_score(x: &Tensor<f32, [?B, 3]>, w: &Tensor<f32, [3, 2]>) -> f32 {
    val total = scores(x, w).sum()
    total / (B * 2) as f32
}

// A struct field may name an extent. It belongs to each value, so one `Batch` type holds
// batches of every size, and the literal checks that its fields agree.
struct Batch {
    images: Tensor<f32, [?B, 3]>,
    labels: Tensor<i32, [?B]>,
}

func make_batch(n: u64) -> Batch {
    // A constructor takes one `dims:` entry per open axis.
    mut images = Tensor::<f32, [?, 3]>::zeros(dims: [n])
    mut labels = Tensor::<i32, [?]>::zeros(dims: [n])
    mut i: u64 = 0
    while i < n {
        images[i, 0] = (i + 1) as f32
        images[i, 1] = 1.0
        labels[i] = (i % 2) as i32
        i += 1
    }
    Batch { images: images, labels: labels }
}

// A run-time check turns a dynamic tensor back into a static one: `None` when the extents
// differ, consuming the tensor either way.
func first_pair(images: Tensor<f32, [?, 3]>) -> f32 {
    val Some(pair) = images.as_shape::<[2, 3]>() else {
        return -1.0
    }
    pair[0, 0] + pair[1, 0]
}

func main() -> i32 {
    val w: Tensor<f32, [3, 2]> = [
        [1.0, 0.0],
        [0.0, 1.0],
        [1.0, 1.0]
    ]

    val small = make_batch(2)
    val large = make_batch(5)
    println("small mean score = {mean_score(&small.images, &w)}")
    println("large mean score = {mean_score(&large.images, &w)}")
    println("large labels sum = {large.labels.sum()}")

    // A fresh name in `.as_shape` stays in scope after it, holding the extent.
    val loaded = Tensor::<f32, [?, 3]>::ones(dims: [4])
    val Some(batch) = loaded.as_shape::<[?N, 3]>() else {
        return 1
    }
    val doubled = &batch + &batch
    println("N = {N}, doubled sum = {doubled.sum()}")

    // Rows of 3 reshape to rows of 6 when the count allows; the count is checked at run time.
    val wide = doubled.reshape([-1, 6])
    println("wide sum = {wide.sum()}")

    val two = Tensor::<f32, [?, 3]>::ones(dims: [2])
    val five = Tensor::<f32, [?, 3]>::ones(dims: [5])
    println("first_pair(two) = {first_pair(two)}")
    println("first_pair(five) = {first_pair(five)}")

    return N as i32 * 10 + 2
}
