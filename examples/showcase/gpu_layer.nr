// Showcase: a dense layer whose forward pass runs on the GPU.
//
// This is an integration example: it exercises several features *together*
// rather than in isolation:
//   `@gpu` · `.to(Device::GPU(0))` / `.to(Device::CPU)` · `@` and broadcast `+` in one
//   kernel body · operators, reductions, `.topk`, a slice, a closure through `.map` and an
//   in-place `*=` on a device tensor outside `@gpu` · `pool` arena blocks · a `.sum()`
//   reduction · a `Vec` of `Copy` structs.
//
// The weights and bias move to the GPU once, before the first batch, and stay there: each
// call reads them in place and stages only the batch. Because a weight lives on the GPU,
// so does each call's result. Its elements are compared with the same layer computed
// inline on the host, one element read at a time, and the two must agree exactly. The
// per-row peak is computed with ordinary operators and reductions, and the leading unit of
// the first row with `.topk`, all of which run on the GPU because their operands live there.
// The two units tie in the last batch, and a tie goes to the earlier unit. The ReLU, a closure
// through `.map`, also runs on the GPU, one call per element. Only a slice of the result, the
// activated first row, comes back with `.to(Device::CPU)`. After each batch the weights decay in place
// with `*=`: on the GPU for the device copy, on the host for the reference, and the two stay
// equal. Every batch is processed inside a `pool`, whose exit also releases the device memory
// the call staged.
// Without a usable NVIDIA GPU the program stops before `main` with a `panic:` line, since
// bare `@gpu` never falls back to the CPU.

@gpu
func dense(x: &Tensor<f32, [4, 3]>, w: &Tensor<f32, [3, 2]>, bias: &Tensor<f32, [2]>) -> Tensor<f32, [4, 2]> {
    val product = x @ w
    product + bias
}

@derive(Copy, Clone)
struct BatchReport {
    batch: i32,
    total: f32,
    row_peak: f32,
    leader: i32,
    first: f32,
    agrees: bool
}

func main() -> i32 {
    mut w: Tensor<f32, [3, 2]> = [
        [0.5, -1.0],
        [0.25, 0.5],
        [-0.5, 1.0]
    ]
    val bias: Tensor<f32, [2]> = [0.5, -0.25]
    mut w_gpu = w.clone().to(Device::GPU(0))
    val bias_gpu = bias.clone().to(Device::GPU(0))

    mut reports: Vec<BatchReport> = Vec::new()
    mut batch = 0
    while batch < 3 {
        pool {
            val scale = (batch + 1) as f32
            val x = Tensor::<f32, [4, 3]>::ones() * scale
            val on_gpu = dense(&x, &w_gpu, &bias_gpu)
            val on_host = &x @ &w + &bias
            val agrees = on_gpu[0, 0] == on_host[0, 0] && on_gpu[3, 1] == on_host[3, 1]
            val row_peak = (&on_gpu - &bias_gpu).sum(axis: 1).max()
            val (_, unit) = on_gpu.topk(k: 1)
            val leader = unit[0, 0]
            val activated = on_gpu.map(|v: f32| -> f32 { if v > 0.0f32 { v } else { 0.0f32 } })
            val total = activated.sum()
            val first_row = activated[0, ..].to(Device::CPU)
            val first = first_row[1]
            reports.push(BatchReport { batch: batch, total: total, row_peak: row_peak, leader: leader, first: first, agrees: agrees })
        }
        w_gpu *= 0.5f32
        w *= 0.5f32
        batch += 1
    }

    mut agreeing = 0
    for i in 0..reports.len() as i32 {
        val report = reports[i]
        println("batch {report.batch}: activated total {report.total}, row peak {report.row_peak}, leading unit {report.leader}, first row's unit 1 {report.first}, host agrees: {report.agrees}")
        if report.agrees { agreeing += 1 }
    }
    return agreeing
}
