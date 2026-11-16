// Showcase: a normalizing layer written once for every rank. Integration example: shape
// parameters (`Tensor<T, S>`, `[..S, N]`, `S.count()`), element types generic under
// `Float`, a generic struct and its `impl`, a trait implemented for every instance, and a
// fixed-step update loop. The same `Affine` serves a feature vector, a matrix and a
// batch of matrices; each shape is its own specialization, checked when it is built. The
// exit code is the sum of the three layers' element counts. Shape parameters alone are in
// `tensors/tensor_shape_parameters.nr`.

trait Summary {
    func cells(&self) -> u64
}

// One scale and one shift per element, of whatever shape the layer is built with.
struct Affine<T, S> {
    scale: Tensor<T, S>,
    shift: Tensor<T, S>
}

impl<T: Float, S> Affine<T, S> {
    func forward(&self, x: &Tensor<T, S>) -> Tensor<T, S> {
        &self.scale * x + &self.shift
    }

    // Move the shift toward `target` by a fixed fraction of the error, in place.
    func nudge(&mut self, x: &Tensor<T, S>, target: &Tensor<T, S>, rate: T) {
        val error = self.forward(x) - target
        self.shift -= error * rate
    }
}

impl<T: Float, S> Summary for Affine<T, S> {
    func cells(&self) -> u64 {
        S.count()
    }
}

// A fresh layer of any shape: unit scale, zero shift.
func identity_layer<T: Float, S>() -> Affine<T, S> {
    Affine { scale: Tensor::<T, S>::ones(), shift: Tensor::<T, S>::zeros() }
}

// A per-feature bias added across every leading axis.
func with_bias<T: Float, S, N>(x: Tensor<T, [..S, N]>, b: &Tensor<T, [N]>) -> Tensor<T, [..S, N]> {
    x + b
}

// Train a layer toward `target` for a fixed number of steps.
func fit<T: Float, S>(layer: &mut Affine<T, S>, x: &Tensor<T, S>, target: &Tensor<T, S>, steps: i32) {
    for _ in 0..steps {
        layer.nudge(x, target, T::from_f64(0.5))
    }
}

func main() -> i32 {
    val x1: Tensor<f32, [4]> = [1.0, 2.0, 3.0, 4.0]
    val t1 = &x1 + 8.0
    mut l1 = identity_layer::<f32, [4]>()
    fit(&mut l1, &x1, &t1, 3)
    // Each step halves the error: 8 -> 4 -> 2 -> 1 left, so the shift is 7.
    println("vector: shift sum {l1.shift.sum()}, output sum {l1.forward(&x1).sum()}")

    val x2: Tensor<f64, [2, 3]> = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]
    val t2 = &x2 * 2.0
    mut l2 = identity_layer::<f64, [2, 3]>()
    fit(&mut l2, &x2, &t2, 4)
    // The error is x, halved four times: the shift is 15/16 of x.
    println("matrix: shift sum {l2.shift.sum()}")

    val bias: Tensor<f32, [3]> = [0.25, 0.5, 0.75]
    val batch = with_bias(Tensor::<f32, [2, 2, 3]>::zeros(), &bias)
    val l3 = identity_layer::<f32, [2, 2, 3]>()
    println("batch: output sum {l3.forward(&batch).sum()}")

    val total = l1.cells() + l2.cells() + l3.cells()
    println("cells: {l1.cells()} + {l2.cells()} + {l3.cells()} = {total}")
    return total as i32
}
