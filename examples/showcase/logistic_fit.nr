// Showcase: logistic regression with an elastic-net penalty, trained by its own gradient.
// Integration example: `.tanh()`, `.log()`, `.abs()` and `.pow(2.0)` inside a `@grad` body,
// the `.backward()`, `.grad()` and `.zero_grad()` loop, `@` sharing the precedence row of
// `*`, a helper function the derivative passes through, reductions, `.item()`, a struct
// recording the run, and `.sqrt()` and `.exp()` in the report. The compiler writes out the
// derivative of each function at compile time. `.abs()` has no slope at 0 and the language
// picks 0 there, so a weight the L1 term drives to exactly zero is not pushed back off it.

// Six samples of two features and a constant column for the bias. Positive features mean
// class 1, negative ones class 0.
func samples() -> Tensor<f32, [6, 3]> {
    [
        [1.0, 2.0, 1.0], [2.0, 0.5, 1.0], [1.5, 1.5, 1.0],
        [-1.0, -2.0, 1.0], [-2.0, -0.5, 1.0], [-0.5, -1.0, 1.0]
    ]
}

func labels() -> Tensor<f32, [6, 1]> {
    [[1.0], [1.0], [1.0], [0.0], [0.0], [0.0]]
}

// sigmoid(z) = (tanh(z / 2) + 1) / 2, which never needs `exp` of a large number.
func probabilities(half_logits: Tensor<f32, [6, 1]>) -> Tensor<f32, [6, 1]> {
    half_logits.tanh() * 0.5 + 0.5
}

@grad(wrt: [w])
func penalized_loss(w: &mut Tensor<f32, [3, 1]>, l1: f32, l2: f32) -> Tensor<f32, []> {
    val y = labels()
    // `*` and `@` share a precedence row, so this halves the inputs before contracting.
    // Halving is exact in binary, so it gives the bits of halving the product.
    val half = samples() * 0.5 @ w
    val p = probabilities(half)
    val likely = &y * p.log()
    val unlikely = (1.0 - y) * (1.0 - p).log()
    val log_likelihood = (likely + unlikely).mean()
    // The bias is not penalized: shrinking it would pull the boundary toward the origin.
    val features: Tensor<f32, [3, 1]> = [[1.0], [1.0], [0.0]]
    val shrunk = w * &features
    val penalty = shrunk.abs().sum() * l1 + shrunk.pow(2.0).sum() * l2
    return Tensor::scalar(penalty - log_likelihood)
}

struct Run {
    first: f32,
    last: f32,
    correct: i32
}

const RATE: f32 = 0.5
const STEPS: i32 = 80

func main() -> i32 {
    mut w: Tensor<f32, [3, 1]> = [[0.0], [0.0], [0.0]]
    mut run = Run { first: 0.0, last: 0.0, correct: 0 }
    for step in 0..STEPS {
        val loss = penalized_loss(&mut w, 0.01, 0.005)
        run.last = loss.item()
        if step == 0 { run.first = run.last }
        loss.backward()
        w -= RATE * w.grad()
        w.zero_grad()
    }

    // Score the trained weights on the training samples: class 1 where the logit is
    // positive, which is where the sigmoid crosses one half.
    val logits = samples() @ &w
    for i in 0..6 {
        val positive = logits[i, 0] > 0.0
        if positive == (i < 3) {
            run.correct += 1
        }
    }

    // Perplexity is the exponential of the mean negative log-likelihood alone, with no
    // penalty: the inverse of the geometric mean probability given to each true label.
    val y = labels()
    val p = probabilities(logits * 0.5)
    val log_likelihood = (&y * p.log() + (1.0 - y) * (1.0 - p).log()).mean()
    val features: Tensor<f32, [3, 1]> = [[1.0], [1.0], [0.0]]
    val norm = (&w * &features).pow(2.0).sum().sqrt()
    println("loss {run.first:.4} -> {run.last:.4}")
    println("weights [{w[0, 0]:.3}, {w[1, 0]:.3}], bias {w[2, 0]:.3}, weight norm {norm:.3}")
    println("perplexity {(-log_likelihood).exp():.3}")
    println("correct {run.correct} of 6")
    return run.correct
}
