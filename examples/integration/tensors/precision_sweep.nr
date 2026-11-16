// One training loop, written once over its element type, run at three precisions.
// Integration example: a generic struct whose `impl` is bounded by `Float`, broadcast
// arithmetic and reductions over a `Tensor<T, [8]>`, float literals and `T::from_f64` in the
// element's own type, `{x:.4}` format specs on a `T`, and a `Vec` of per-precision reports.
// The data lies on y = 2x + 1. `f64` and `f32` land on the line. `bf16` stalls short of it:
// once a step is smaller than half the spacing between `bf16` values near the weight, the
// update rounds back to the weight it started from.

struct Line<T> {
    w: T,
    b: T,
}

impl<T: Float> Line<T> {
    // One gradient step on the mean squared error.
    func step(self, x: &Tensor<T, [8]>, y: &Tensor<T, [8]>, lr: T) -> Line<T> {
        val err = x * self.w + self.b - y
        val half_n = T::from_f64(4.0)
        val gw = (&err * x).sum() / half_n
        val gb = err.sum() / half_n
        Line { w: self.w - lr * gw, b: self.b - lr * gb }
    }

    func loss(&self, x: &Tensor<T, [8]>, y: &Tensor<T, [8]>) -> T {
        val err = x * self.w + self.b - y
        (&err * &err).mean()
    }
}

struct Report {
    name: string,
    on_line: bool,
}

func fit<T: Float>(name: string, steps: i32) -> Report {
    val x = Tensor::<T, [8]>::from([0.0, 0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 1.75])
    val y = &x * 2.0 + 1.0
    mut line = Line { w: T::zero(), b: T::zero() }
    for _ in 0..steps {
        line = line.step(&x, &y, 0.5)
    }
    val loss = line.loss(&x, &y)
    println("{name}: w = {line.w:.4}, b = {line.b:.4}, loss = {loss:.6}")
    val close = (line.w - 2.0).abs() < 0.01 && (line.b - 1.0).abs() < 0.01
    Report { name: name, on_line: close }
}

func main() -> i32 {
    mut reports: Vec<Report> = Vec::new()
    reports.push(fit::<f64>("f64 ", 400))
    reports.push(fit::<f32>("f32 ", 400))
    reports.push(fit::<bf16>("bf16", 400))
    mut landed = 0
    for r in &reports {
        if r.on_line {
            landed += 1
        } else {
            println("{r.name} stalled short of the line")
        }
    }
    landed
}
