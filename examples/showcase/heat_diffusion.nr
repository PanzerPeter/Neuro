// Showcase: heat spreading along a rod, one GPU thread per cell.
//
// This is an integration example: it exercises several features *together*
// rather than in isolation:
//   `@kernel` with `thread_id` and an early `return` · `.to(Device::GPU(0))` /
//   `.to(Device::CPU)` · a `@gpu` function over device tensors · `pool` arena blocks · a
//   closure through `.map` · a `.sum()` reduction · a `Vec` of `Copy` structs.
//
// The rod starts cold with a hot spot in the middle, and both ends are held at their
// starting temperature. Each step, every cell moves toward the average of its neighbours,
// computed by a hand-written kernel: one thread per cell, 32 cells to a block. The two
// temperature buffers live on the GPU the whole time and take turns as input and output,
// so no step copies anything to or from the host. Every 25 steps a `@gpu` function measures
// how far the rod has moved since the last report, that difference comes back to the host
// to be summed, and a second kernel keeps the new temperatures on the GPU for the next
// report. The same simulation also runs on the host, and the two must agree.
// Without a usable NVIDIA GPU the program stops before `main` with a `panic:` line.

@kernel(threads: [32])
func diffuse(t: &Tensor<f32, [100]>, rate: f32, next: &mut Tensor<f32, [100]>) {
    val i = thread_id.x
    if i >= 100 {
        return
    }
    if i == 0 || i == 99 {
        next[i] = t[i]
        return
    }
    val spread = t[i - 1] + t[i + 1] - 2.0f32 * t[i]
    next[i] = t[i] + rate * spread
}

@kernel(threads: [32])
func copy_into(t: &Tensor<f32, [100]>, out: &mut Tensor<f32, [100]>) {
    val i = thread_id.x
    if i < 100 {
        out[i] = t[i]
    }
}

@gpu
func change(now: &Tensor<f32, [100]>, before: &Tensor<f32, [100]>) -> Tensor<f32, [100]> {
    now - before
}

func host_step(t: &Tensor<f32, [100]>, rate: f32) -> Tensor<f32, [100]> {
    mut next: Tensor<f32, [100]> = Tensor::zeros()
    next[0] = t[0]
    next[99] = t[99]
    for i in 1..99 {
        val spread = t[i - 1] + t[i + 1] - 2.0f32 * t[i]
        next[i] = t[i] + rate * spread
    }
    next
}

@derive(Copy, Clone)
struct Report {
    step: i32,
    middle: f32,
    moved: f32
}

func main() -> i32 {
    val rate = 0.25f32
    mut start: Tensor<f32, [100]> = Tensor::zeros()
    start[0] = 20.0f32
    start[99] = 20.0f32
    for i in 45..55 {
        start[i] = 100.0f32
    }

    mut host = start.clone()
    mut a = start.clone().to(Device::GPU(0))
    mut b = Tensor::<f32, [100]>::zeros().to(Device::GPU(0))
    mut last = start.to(Device::GPU(0))
    val magnitude = |x: f32| if x < 0.0f32 { -x } else { x }

    mut reports: Vec<Report> = Vec::new()
    mut step = 0
    while step < 100 {
        pool {
            // Even steps read `a` and write `b`, odd steps the other way round.
            if step % 2 == 0 {
                diffuse(&a, rate, &mut b)
            } else {
                diffuse(&b, rate, &mut a)
            }
            host = host_step(&host, rate)
        }
        step += 1
        if step % 25 == 0 {
            // After an odd number of steps the newest temperatures are in `b`, after an
            // even number in `a`; 25, 50, 75 and 100 alternate.
            val moved = if step % 2 == 0 {
                change(&a, &last).to(Device::CPU).map(magnitude).sum()
            } else {
                change(&b, &last).to(Device::CPU).map(magnitude).sum()
            }
            if step % 2 == 0 {
                copy_into(&a, &mut last)
            } else {
                copy_into(&b, &mut last)
            }
            reports.push(Report { step: step, middle: host[50], moved: moved })
        }
    }

    for i in 0..reports.len() as i32 {
        val report = reports[i]
        println("step {report.step}: middle {report.middle}, moved {report.moved} since the last report")
    }
    // 100 steps is an even number, so the newest temperatures are in `a`.
    val gpu = a.to(Device::CPU)
    mut agree = 0
    for i in 0..100 {
        if gpu[i] == host[i] { agree += 1 }
    }
    println("cells agreeing with the host: {agree} of 100")
    return agree
}
