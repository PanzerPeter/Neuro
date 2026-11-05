// Showcase: train on the CPU, predict on the GPU, in one short program. Integration example:
// a `@grad` loss with a `.backward()` loop, a `@gpu` forward pass over the trained weights
// moved with `.to(Device::GPU(0))`, and shapes the compiler checks in both. This is the program
// in the README's demo recording. Without a usable NVIDIA GPU it stops before `main` with a
// `panic:` line, since bare `@gpu` never falls back to the CPU.

// The targets are exactly `x @ [1, 2]`, so the loss reaches zero at that weight.
@grad
func loss(w: &mut Tensor<f32, [2, 1]>) -> Tensor<f32, []> {
    val x: Tensor<f32, [4, 2]> = [[1.0, 2.0], [2.0, 0.0], [0.0, 1.0], [3.0, 1.0]]
    val y: Tensor<f32, [4, 1]> = [[5.0], [2.0], [2.0], [5.0]]
    val miss = x @ w - y
    return Tensor::scalar((&miss * &miss).mean())
}

@gpu
func predict(x: &Tensor<f32, [1, 2]>, w: &Tensor<f32, [2, 1]>) -> Tensor<f32, [1, 1]> {
    x @ w
}

func main() -> i32 {
    mut w: Tensor<f32, [2, 1]> = [[0.0], [0.0]]
    for step in 0..61 {
        val error = loss(&mut w)
        error.backward()
        w -= 0.05f32 * w.grad()
        w.zero_grad()
        if step % 15 == 0 {
            val measured = error.item()
            println("step {step:>2}: loss {measured:>8.4}")
        }
    }
    println("learned w = [{w[0, 0]:.2}, {w[1, 0]:.2}]")

    val x: Tensor<f32, [1, 2]> = [[1.0, 1.0]]
    val w_gpu = w.clone().to(Device::GPU(0))
    val y = predict(&x, &w_gpu).to(Device::CPU)
    println("predicted on the GPU: {y[0, 0]:.2}")
    return (y[0, 0] + 0.5f32) as i32
}
