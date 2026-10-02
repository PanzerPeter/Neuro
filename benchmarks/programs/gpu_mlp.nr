// Inference through a two-layer perceptron on the GPU: `relu(x @ w1 + b1) @ w2 + b2` in
// one `@gpu` function, a [1024, 1024] batch through a 1024-unit hidden layer to 256
// outputs, twenty rounds, with one input element bumped between rounds so no round can
// be hoisted. The weights and biases move to the GPU once; each batch is staged from
// the host and its outputs come back for the checksum, as a serving loop would. The
// ReLU is `(h + |h|) * half`, exact on every value. `half` is a parameter rather than a
// `0.5f32` because a `@gpu` body does not lower a scalar literal operand yet. Every
// input, weight and bias is a small integer, so every partial sum stays below 2^24 and
// is exact in f32, which is what lets the checksum match NumPy's and PyTorch's to the
// digit. `.abs()` links libdevice, so compiling needs the CUDA toolkit; running needs
// an NVIDIA GPU. The harness skips the row without one.

@gpu
func forward(x: &Tensor<f32, [1024, 1024]>, w1: &Tensor<f32, [1024, 1024]>, b1: &Tensor<f32, [1024]>, w2: &Tensor<f32, [1024, 256]>, b2: &Tensor<f32, [256]>, half: f32) -> Tensor<f32, [1024, 256]> {
    val h = x @ w1 + b1
    val magnitude = h.abs()
    val activated = (h + magnitude) * half
    activated @ w2 + b2
}

func main() -> i32 {
    mut x = Tensor::<f32, [1024, 1024]>::zeros()
    mut w1 = Tensor::<f32, [1024, 1024]>::zeros()
    mut b1 = Tensor::<f32, [1024]>::zeros()
    mut w2 = Tensor::<f32, [1024, 256]>::zeros()
    mut b2 = Tensor::<f32, [256]>::zeros()
    for i in 0..1024 {
        for j in 0..1024 {
            x[i, j] = ((i * 7 + j * 3) % 4) as f32
            w1[i, j] = ((i * 5 + j * 11) % 5 - 2) as f32
        }
        for j in 0..256 {
            w2[i, j] = ((i * 3 + j * 13) % 3 - 1) as f32
        }
        b1[i] = (i % 7 - 3) as f32
    }
    for j in 0..256 {
        b2[j] = (j % 5 - 2) as f32
    }
    val w1_gpu = w1.to(Device::GPU(0))
    val b1_gpu = b1.to(Device::GPU(0))
    val w2_gpu = w2.to(Device::GPU(0))
    val b2_gpu = b2.to(Device::GPU(0))
    mut checksum: i64 = 0
    for r in 0..20 {
        x[r, r] = x[r, r] + 1.0f32
        val out = forward(&x, &w1_gpu, &b1_gpu, &w2_gpu, &b2_gpu, 0.5f32).to(Device::CPU)
        for i in 0..1024 {
            for j in 0..256 {
                checksum = checksum + out[i, j] as i64
            }
        }
    }
    println("checksum = {checksum}")
    return 0
}
