// Tensor values: literal coercion and the construction helpers
//
// `Tensor<T, [d0, d1, ...]>` carries its element type and every extent in the type,
// so two tensors of different shapes are different types. A nested array literal
// becomes a tensor only where a `Tensor<...>` annotation says so; with no annotation
// it stays a plain array. Without an annotation, name the type with a turbofish.
//
// A tensor owns its buffer and is not `Copy`, so binding it or passing it MOVES it.
// Reading elements back out is slicing and indexing, which is not implemented yet —
// these programs build tensors and pass them around.

// A tensor parameter names its full shape; a caller with a different shape is a
// compile error, not a runtime one.
func accept_batch(batch: Tensor<f32, [2, 3]>) -> i32 {
    return 6
}

struct Layer {
    weights: Tensor<f32, [4, 4]>,
    bias: Tensor<f32, [4]>
}

func build_layer() -> Layer {
    return Layer {
        weights: Tensor::<f32, [4, 4]>::identity(),
        bias: Tensor::<f32, [4]>::zeros()
    }
}

func main() -> i32 {
    // The annotation drives the coercion, and it types the elements too: these are
    // `f32` literals, not `f64` ones being narrowed.
    val row: Tensor<f32, [3]> = [1.0, 2.0, 3.0]

    // Nested literals must be rectangular — every row here is 3 wide because the
    // shape says so. A ragged literal is a compile error.
    val grid: Tensor<f32, [2, 3]> = [
        [1.0, 2.0, 3.0],
        [4.0, 5.0, 6.0]
    ]

    // Rank 3, and any element type that is a fixed-width scalar.
    val cube: Tensor<i32, [2, 2, 2]> = [
        [[1, 2], [3, 4]],
        [[5, 6], [7, 8]]
    ]

    // With no annotation in scope, name the type with a turbofish instead.
    val zeros = Tensor::<f32, [3, 3]>::zeros()
    val ones = Tensor::<f32, [3, 3]>::ones()
    val eye = Tensor::<f32, [4, 4]>::identity()
    val from_literal = Tensor::<f32, [3]>::from([1.0, 2.0, 3.0])

    // Weight initialization. The generator is seeded from a fixed constant, so a
    // compiled program draws the same weights on every run.
    val weights = Tensor::<f32, [8, 4]>::random_normal(mean: 0.0f32, std: 0.02f32)

    // A rank-0 (scalar) tensor has no array-literal form — it is built directly.
    val loss: Tensor<f32, []> = Tensor::scalar(0.5)

    // Without a `Tensor` annotation this is a plain `[f64; 3]`, indexable as one.
    val plain = [10.0, 20.0, 30.0]
    val third = plain[2] as i32

    val layer = build_layer()

    println("row       = Tensor<f32, [3]>")
    println("grid      = Tensor<f32, [2, 3]>")
    println("cube      = Tensor<i32, [2, 2, 2]>")
    println("eye       = Tensor<f32, [4, 4]>::identity()")
    println("weights   = Tensor<f32, [8, 4]>::random_normal(0.0, 0.02)")
    println("loss      = Tensor<f32, []>::scalar(0.5)")
    println("plain[2]  = {third}   (a plain array, not a tensor)")

    // `grid` moves into the call; it is not usable afterwards.
    return accept_batch(grid)
}
