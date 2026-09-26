// Showcase: Newton's method against gradient descent, on one logistic fit.
//
// This is an integration example: it exercises several features *together*
// rather than in isolation:
//   Phase 3's `@grad(order: 2)`, whose `.backward()` fills `.hessian()` beside
//   `.grad()` · elementwise `.exp()` and `.log()` inside the differentiated body,
//   each differentiated twice · 2D's `pool` arena around every step · element
//   reads of the gradient and the Hessian, and a tensor literal built from them ·
//   a `Copy` struct recording each run · string interpolation with the format
//   mini-language.
//
// Both optimizers minimize the same regularized logistic loss over a weight and a
// bias. Gradient descent follows the slope; Newton's method also reads the
// curvature, solving the 2 x 2 system `H d = g` for its step, and lands on the
// minimum in a handful of steps where descent needs ten times as many.

// Six points on a line, their classes overlapping, so the best fit is finite. The
// ridge term makes the loss strictly convex, which Newton's method relies on.
@grad(order: 2)
func logistic_loss(theta: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    val x: Tensor<f32, [6]> = [-2.0, -1.0, -0.5, 0.5, 1.0, 2.5]
    val y: Tensor<f32, [6]> = [-1.0, -1.0, 1.0, -1.0, 1.0, 1.0]
    val weight = theta[0]
    val bias = theta[1]
    val margin = (x * weight + bias) * y
    val loss = ((margin * -1.0).exp() + 1.0).log()
    val ridge = theta * theta
    return Tensor::scalar(loss.mean() + ridge.sum() * 0.05)
}

@derive(Copy, Clone)
struct Fit {
    steps: i32,
    loss: f32,
    weight: f32,
    bias: f32
}

// The slope below which a fit counts as converged.
const TOLERANCE: f32 = 0.0001

func newton() -> Fit {
    mut theta: Tensor<f32, [2]> = [0.0, 0.0]
    mut steps = 0
    mut loss = 0.0f32
    mut slope = 1.0f32
    while slope > TOLERANCE {
        pool {
            val l = logistic_loss(&mut theta)
            l.backward()
            loss = l.sum()
            val (g0, g1) = (theta.grad()[0], theta.grad()[1])
            val (a, b) = (theta.hessian()[0, 0], theta.hessian()[0, 1])
            val (c, d) = (theta.hessian()[1, 0], theta.hessian()[1, 1])
            slope = g0.abs() + g1.abs()
            // The inverse of a 2 x 2 matrix, applied to the gradient.
            val det = a * d - b * c
            val step: Tensor<f32, [2]> = [(d * g0 - b * g1) / det, (a * g1 - c * g0) / det]
            theta -= step
        }
        steps += 1
    }
    return Fit { steps: steps, loss: loss, weight: theta[0], bias: theta[1] }
}

func descent(rate: f32) -> Fit {
    mut theta: Tensor<f32, [2]> = [0.0, 0.0]
    mut steps = 0
    mut loss = 0.0f32
    mut slope = 1.0f32
    while slope > TOLERANCE {
        pool {
            val l = logistic_loss(&mut theta)
            l.backward()
            loss = l.sum()
            slope = theta.grad()[0].abs() + theta.grad()[1].abs()
            theta -= rate * theta.grad()
            theta.zero_grad()
        }
        steps += 1
    }
    return Fit { steps: steps, loss: loss, weight: theta[0], bias: theta[1] }
}

func main() -> i32 {
    val fast = newton()
    val slow = descent(0.5)
    for fit in [fast, slow] {
        println("{fit.steps:>4} steps: loss {fit.loss:.5}, weight {fit.weight:.4}, bias {fit.bias:.4}")
    }
    // Both reach the same minimum; only the number of steps differs.
    assert((fast.weight - slow.weight).abs() < 0.001f32)
    assert((fast.bias - slow.bias).abs() < 0.001f32)
    return fast.steps * 50 + slow.steps
}
