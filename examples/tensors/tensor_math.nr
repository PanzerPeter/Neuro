// Elementwise math: `.exp()`, `.log()`, `.sqrt()`, `.tanh()`, `.abs()` and `.pow(p)`. The
// six are methods on every float scalar and on a float tensor. A tensor applies them
// element by element into a fresh tensor of the same shape, and a scalar returns a scalar.
// `.pow(p)` takes its exponent as a scalar of the element type. The receiver is read, not
// moved. Out-of-domain inputs follow IEEE 754 and are not diagnosed: `.log()` of 0 is
// `-inf`, and `.sqrt()` of a negative is NaN. `f16` and `bf16` compute in `f32` and round
// once. In a `@grad` body each is differentiated by its own rule (see
// `showcase/logistic_fit.nr`).

// A borrowed receiver: `x` stays with the caller.

func softplus_total(x: &Tensor<f64, [3]>) -> f64 {
    (x.exp() + 1.0).log().sum()
}

func main() -> i32 {
    val a: f64 = 3.0
    val b: f64 = 4.0
    val hypotenuse = (a * a + b * b).sqrt()
    println("hypotenuse {hypotenuse}")
    println("e^1 = {(1.0).exp():.6}, ln 10 = {(10.0).log():.6}, 2^10 = {(2.0).pow(10.0)}")

    val x: Tensor<f64, [3]> = [-1.0, 0.0, 2.0]
    val magnitude = x.abs()
    val squashed = x.tanh()
    val squared = x.pow(2.0)
    println("abs   [{magnitude[0]}, {magnitude[1]}, {magnitude[2]}]")
    println("tanh  [{squashed[0]:.4}, {squashed[1]:.4}, {squashed[2]:.4}]")
    println("x^2   [{squared[0]}, {squared[1]}, {squared[2]}]")
    println("softplus total {softplus_total(&x):.4}, x still sums to {x.sum()}")

    val halves: Tensor<bf16, [2]> = [9.0bf16, 25.0bf16]
    val roots = halves.sqrt()
    println("bf16 roots [{roots[0] as f32}, {roots[1] as f32}]")

    val zero: f64 = 0.0
    println("log 0 = {zero.log()}, sqrt -1 is NaN: {(zero - 1.0).sqrt().is_nan()}")
    return hypotenuse as i32 + (roots[1] as f32) as i32
}
