// Showcase: a two-layer classifier whose layers run on the GPU when one is usable.
//
// This is an integration example: it exercises several features *together*
// rather than in isolation:
//   `@gpu(fallback: true)` · `@` and broadcast `+` in kernel bodies · `pool` arena blocks
//   · a closure through `.map` · an enum with a payload and `match`
//   · a `Vec` of `Copy` structs.
//
// Both layers carry `fallback: true`, so the program picks their bodies once at startup:
// the GPU kernels where an NVIDIA GPU is usable, the same arithmetic on the CPU where
// none is (or where neurc was built without MLIR). The output is identical either way.
// Each batch is also computed inline on the host, and a `Verdict` says whether the two
// agree. Every batch runs inside a `pool`.

@gpu(fallback: true)
func hidden(x: &Tensor<f32, [4, 3]>, w: &Tensor<f32, [3, 5]>, b: &Tensor<f32, [5]>) -> Tensor<f32, [4, 5]> {
    val product = x @ w
    product + b
}

@gpu(fallback: true)
func logits(h: &Tensor<f32, [4, 5]>, w: &Tensor<f32, [5, 2]>) -> Tensor<f32, [4, 2]> {
    h @ w
}

enum Verdict {
    Agrees,
    DiffersAtRow(i32)
}

func compare(a: &Tensor<f32, [4, 2]>, b: &Tensor<f32, [4, 2]>) -> Verdict {
    for row in 0..4 {
        if a[row, 0] != b[row, 0] || a[row, 1] != b[row, 1] {
            return Verdict::DiffersAtRow(row)
        }
    }
    return Verdict::Agrees
}

@derive(Copy, Clone)
struct BatchReport {
    batch: i32,
    positives: i32,
    total: f32,
    agrees: bool
}

func main() -> i32 {
    val x: Tensor<f32, [4, 3]> = [
        [1.0, -1.0, 0.5],
        [0.0, 2.0, -1.0],
        [-1.5, 0.5, 1.0],
        [2.0, 0.0, 0.25]
    ]
    val w1: Tensor<f32, [3, 5]> = [
        [0.5, -0.25, 1.0, 0.0, -0.5],
        [0.25, 0.5, -0.5, 1.0, 0.0],
        [-1.0, 0.75, 0.25, -0.25, 0.5]
    ]
    val b1: Tensor<f32, [5]> = [0.25, -0.5, 0.0, 0.5, 0.25]
    val w2: Tensor<f32, [5, 2]> = [
        [1.0, -0.5],
        [-0.5, 1.0],
        [0.25, 0.25],
        [0.0, 0.75],
        [-0.75, 0.5]
    ]
    val relu = |v: f32| if v > 0.0f32 { v } else { 0.0f32 }

    mut reports: Vec<BatchReport> = Vec::new()
    mut batch = 0
    while batch < 3 {
        pool {
            val scaled = &x * (batch + 1) as f32
            val h = hidden(&scaled, &w1, &b1).map(relu)
            val out = logits(&h, &w2)
            val inline = (&scaled @ &w1 + &b1).map(relu) @ &w2
            val agrees = match compare(&out, &inline) {
                Verdict::Agrees => true,
                Verdict::DiffersAtRow(row) => {
                    println("batch {batch}: row {row} differs from the host")
                    false
                }
            }
            mut positives = 0
            for row in 0..4 {
                if out[row, 1] > out[row, 0] { positives += 1 }
            }
            val total = h.sum()
            reports.push(BatchReport { batch: batch, positives: positives, total: total, agrees: agrees })
        }
        batch += 1
    }

    mut agreeing = 0
    for i in 0..reports.len() as i32 {
        val report = reports[i]
        println("batch {report.batch}: {report.positives} of 4 rows in class 1, hidden total {report.total}, host agrees: {report.agrees}")
        if report.agrees { agreeing += 1 }
    }
    return agreeing
}
