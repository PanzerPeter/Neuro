// Showcase: a dense layer whose forward pass runs on the GPU.
//
// This is an integration example: it exercises several features *together*
// rather than in isolation:
//   `@gpu` · `.to(Device::GPU(0))` / `.to(Device::CPU)` · `@` and broadcast `+` in one
//   kernel body · `pool` arena blocks · a closure through `.map` · a `.sum()` reduction
//   · a `Vec` of `Copy` structs.
//
// The weights and bias move to the GPU once, before the first batch, and stay there: each
// call reads them in place and stages only the batch. Because a weight lives on the GPU,
// so does each call's result, and `.to(Device::CPU)` brings it back for the ReLU and totals,
// which run on the host. The same layer is also computed inline on the host, and the two
// must agree exactly. Every batch is processed inside a `pool`, whose exit also releases
// the device memory the call staged. Without a usable NVIDIA GPU the program stops before
// `main` with a `panic:` line, since bare `@gpu` never falls back to the CPU.

@gpu
func dense(x: &Tensor<f32, [4, 3]>, w: &Tensor<f32, [3, 2]>, bias: &Tensor<f32, [2]>) -> Tensor<f32, [4, 2]> {
    val product = x @ w
    product + bias
}

@derive(Copy, Clone)
struct BatchReport {
    batch: i32,
    total: f32,
    agrees: bool
}

func main() -> i32 {
    val w: Tensor<f32, [3, 2]> = [
        [0.5, -1.0],
        [0.25, 0.5],
        [-0.5, 1.0]
    ]
    val bias: Tensor<f32, [2]> = [0.5, -0.25]
    val w_gpu = w.clone().to(Device::GPU(0))
    val bias_gpu = bias.clone().to(Device::GPU(0))
    val relu = |v: f32| if v > 0.0f32 { v } else { 0.0f32 }

    mut reports: Vec<BatchReport> = Vec::new()
    mut batch = 0
    while batch < 3 {
        pool {
            val scale = (batch + 1) as f32
            val x = Tensor::<f32, [4, 3]>::ones() * scale
            val on_gpu = dense(&x, &w_gpu, &bias_gpu).to(Device::CPU)
            val on_host = &x @ &w + &bias
            val agrees = on_gpu[0, 0] == on_host[0, 0] && on_gpu[3, 1] == on_host[3, 1]
            val activated = on_gpu.map(relu)
            val total = activated.sum()
            reports.push(BatchReport { batch: batch, total: total, agrees: agrees })
        }
        batch += 1
    }

    mut agreeing = 0
    for i in 0..reports.len() as i32 {
        val report = reports[i]
        println("batch {report.batch}: activated total {report.total}, host agrees: {report.agrees}")
        if report.agrees { agreeing += 1 }
    }
    return agreeing
}
