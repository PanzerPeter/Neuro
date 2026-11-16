// Module `data`: the training and holdout sets. A `Batch` holds six samples of two
// features, and that shape is in its field types: every function in another module that
// takes one agrees on the shape without a runtime check.

export struct Batch {
    export x: Tensor<f32, [6, 2]>,
    export y: Tensor<f32, [6, 1]>
}

// Every target is exactly `2 * x0 - x1 + 0.5`, so a perfect fit exists.
export func training_set() -> Batch {
    val x: Tensor<f32, [6, 2]> = [[1.0, 2.0], [2.0, 0.0], [0.0, 1.0], [3.0, 1.0], [1.0, 0.0], [2.0, 2.0]]
    val y: Tensor<f32, [6, 1]> = [[0.5], [4.5], [-0.5], [5.5], [2.5], [2.5]]
    Batch { x: x, y: y }
}

// Samples the model never trains on.
export func holdout() -> Batch {
    val x: Tensor<f32, [6, 2]> = [[4.0, 1.0], [0.0, 0.0], [1.0, 3.0], [5.0, 2.0], [0.0, 2.0], [3.0, 3.0]]
    val y: Tensor<f32, [6, 1]> = [[7.5], [0.5], [-0.5], [8.5], [-1.5], [3.5]]
    Batch { x: x, y: y }
}
