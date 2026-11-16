// A training run that starts from a file, stops, and resumes on the GPU.
// Integration example: `std::safetensors` reads a fixed batch and writes a checkpoint with the
// step count in its metadata, `@grad` over two parameters trains a linear model, the resumed
// half moves its tensors with `.to(Device::GPU(0))` and trains there, a `@gpu` function scores
// the result, `?` carries an `IoError` out of the run, and `std::fs` removes the files. The
// GPU half must land on the same bits as the same steps on the host: the loss squares with
// `*`, which is exact on both, where `.pow(2.0)` would call each side's own math library.
// The getting-started tutorial writes the batch from Python; here `write_batch` stands in.
// Without a usable NVIDIA GPU the program stops before `main`, since bare `@gpu` never falls
// back to the CPU.

const RATE: f32 = 0.1
const HOST_STEPS: i32 = 6
const GPU_STEPS: i32 = 6

@grad(wrt: [w, b])
func loss(
    w: &mut Tensor<f32, [3, 1]>,
    b: &mut Tensor<f32, [1]>,
    x: &Tensor<f32, [8, 3]>,
    y: &Tensor<f32, [8, 1]>
) -> Tensor<f32, []> {
    val error = x @ w + b - y
    return Tensor::scalar((&error * &error).mean())
}

@gpu
func predict(x: &Tensor<f32, [8, 3]>, w: &Tensor<f32, [3, 1]>, b: &Tensor<f32, [1]>) -> Tensor<f32, [8, 1]> {
    val product = x @ w
    product + b
}

@derive(Copy, Clone)
struct Step {
    index: i32,
    loss: f32
}

// One gradient step on whichever device the tensors live: the loss before it.
func step(
    w: &mut Tensor<f32, [3, 1]>,
    b: &mut Tensor<f32, [1]>,
    x: &Tensor<f32, [8, 3]>,
    y: &Tensor<f32, [8, 1]>
) -> f32 {
    val l = loss(w, b, x, y)
    val value = l.item()
    l.backward()
    *w -= RATE * w.grad()
    *b -= RATE * b.grad()
    w.zero_grad()
    b.zero_grad()
    value
}

func write_batch(path: &string) -> Result<(), IoError> {
    val x: Tensor<f32, [8, 3]> = [
        [1.0, 2.0, 0.5], [0.5, -1.0, 2.0], [1.5, 0.25, -0.5], [2.0, 1.0, 1.0],
        [-1.0, 0.5, 0.75], [0.0, 1.5, -1.0], [1.25, -0.5, 0.0], [-0.5, -1.5, 1.5]
    ]
    val y: Tensor<f32, [8, 1]> = [[1.0], [2.0], [0.5], [3.0], [-1.0], [0.25], [1.5], [-0.5]]
    mut out = std::safetensors::Writer::new()
    out.add("x", &x)
    out.add("y", &y)
    out.save(path)
}

func save(path: &string, w: &Tensor<f32, [3, 1]>, b: &Tensor<f32, [1]>, done: i32) -> Result<(), IoError> {
    mut out = std::safetensors::Writer::new()
    out.add("w", w)
    out.add("b", b)
    val text = "{done}"
    out.add_metadata("step", &text)
    out.save(path)
}

func run(batch_path: &string, checkpoint: &string) -> Result<i32, IoError> {
    write_batch(batch_path)?
    val batch = std::safetensors::open(batch_path)?
    val x: Tensor<f32, [8, 3]> = batch.tensor("x")?
    val y: Tensor<f32, [8, 1]> = batch.tensor("y")?

    mut history: Vec<Step> = Vec::new()
    mut w: Tensor<f32, [3, 1]> = Tensor::zeros()
    mut b: Tensor<f32, [1]> = Tensor::zeros()
    for i in 0..HOST_STEPS {
        history.push(Step { index: i, loss: step(&mut w, &mut b, &x, &y) })
    }
    save(checkpoint, &w, &b, HOST_STEPS)?

    val saved = std::safetensors::open(checkpoint)?
    val done = (saved.metadata("step") ?? "0").parse::<i32>() ?? 0
    val w_saved: Tensor<f32, [3, 1]> = saved.tensor("w")?
    val b_saved: Tensor<f32, [1]> = saved.tensor("b")?

    // The same steps twice from the checkpoint: once on the host, once on the GPU.
    mut w_host = w_saved.clone()
    mut b_host = b_saved.clone()
    val gpu = Device::GPU(0)
    mut w_gpu = w_saved.to(gpu)
    mut b_gpu = b_saved.to(gpu)
    val x_gpu = x.clone().to(gpu)
    val y_gpu = y.clone().to(gpu)
    mut same = true
    for i in done..done + GPU_STEPS {
        val on_host = step(&mut w_host, &mut b_host, &x, &y)
        val on_gpu = step(&mut w_gpu, &mut b_gpu, &x_gpu, &y_gpu)
        same = same && on_host == on_gpu
        history.push(Step { index: i, loss: on_gpu })
    }

    for s in &history {
        val place = if s.index < done { "host" } else { "gpu " }
        println("{place} step {s.index}: loss {s.loss:.6}")
    }
    val w_back = w_gpu.clone().to(Device::CPU)
    val b_back = b_gpu.clone().to(Device::CPU)
    for i in 0..3 {
        same = same && w_back[i, 0] == w_host[i, 0]
    }
    same = same && b_back[0] == b_host[0]
    println("the GPU half matches the host bit for bit: {same}")

    val predicted = predict(&x_gpu, &w_gpu, &b_gpu).to(Device::CPU)
    val miss = (&predicted - &y).abs()
    println("largest miss {miss.max():.3}, mean {miss.mean():.3}")
    println("w = [{w_back[0, 0]:.4}, {w_back[1, 0]:.4}, {w_back[2, 0]:.4}], b = {b_back[0]:.4}")
    Ok(history.len() as i32)
}

func main() -> i32 {
    val batch_path: &string = "neuro_showcase_batch.safetensors"
    val checkpoint: &string = "neuro_showcase_run.safetensors"
    val steps = run(batch_path, checkpoint)
    std::fs::remove_file(batch_path) ?? panic("cannot remove the batch")
    std::fs::remove_file(checkpoint) ?? panic("cannot remove the checkpoint")
    match steps {
        Ok(count) => count,
        Err(e) => {
            println("error: {e.message}")
            -1
        }
    }
}
