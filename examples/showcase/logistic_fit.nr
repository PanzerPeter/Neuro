// Showcase: logistic regression with an elastic-net penalty, trained by its own gradient.
//
// This is an integration example: it exercises several features *together*
// rather than in isolation:
//   Phase 3's elementwise math inside a `@grad` body: `.tanh()` builds the sigmoid,
//   `.log()` the likelihood, `.abs()` and `.pow(2.0)` the penalty, each differentiated
//   by its own rule · Phase 3's `.backward()` / `.grad()` / `.zero_grad()` training loop
//   · 2C's `@` contraction and scalar broadcast · 2B's reductions and in-place compound
//   assignment · a struct recording the run · string interpolation with the format
//   mini-language, where `.sqrt()` and `.exp()` turn the results into readable numbers.
//
// The derivative of every one of those functions is written out by the compiler at
// compile time. `.abs()` has no slope at 0, and the language picks 0 there, so a weight
// the L1 term drives to exactly zero is not pushed back off it.

// Six samples of two features and a constant column for the bias. Positive
// features mean class 1, negative ones class 0.
@grad(wrt: [w])
func penalized_loss(w: &mut Tensor<f32, [3, 1]>, l1: f32, l2: f32) -> Tensor<f32, []> {
    val x: Tensor<f32, [6, 3]> = [
        [1.0, 2.0, 1.0], [2.0, 0.5, 1.0], [1.5, 1.5, 1.0],
        [-1.0, -2.0, 1.0], [-2.0, -0.5, 1.0], [-0.5, -1.0, 1.0]
    ]
    val y: Tensor<f32, [6, 1]> = [[1.0], [1.0], [1.0], [0.0], [0.0], [0.0]]
    // sigmoid(z) = (tanh(z / 2) + 1) / 2, which never needs `exp` of a large number.
    val half = (x @ w) * 0.5
    val p = half.tanh() * 0.5 + 0.5
    val likely = &y * p.log()
    val unlikely = (1.0 - y) * (1.0 - p).log()
    val log_likelihood = (likely + unlikely).mean()
    val penalty = w.abs().sum() * l1 + w.pow(2.0).sum() * l2
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
        loss.backward()
        w -= RATE * w.grad()
        w.zero_grad()
        run.last = loss.sum()
        if step == 0 { run.first = run.last }
    }

    // Score the trained weights on the training samples: class 1 where the logit is
    // positive, which is where the sigmoid crosses one half.
    val x: Tensor<f32, [6, 3]> = [
        [1.0, 2.0, 1.0], [2.0, 0.5, 1.0], [1.5, 1.5, 1.0],
        [-1.0, -2.0, 1.0], [-2.0, -0.5, 1.0], [-0.5, -1.0, 1.0]
    ]
    val logits = x @ &w
    for i in 0..6 {
        val positive = logits[i as u64, 0] > 0.0
        if positive == (i < 3) {
            run.correct += 1
        }
    }

    val norm = w.pow(2.0).sum().sqrt()
    println("loss {run.first:.4} -> {run.last:.4}")
    println("weights [{w[0, 0]:.3}, {w[1, 0]:.3}], bias {w[2, 0]:.3}, norm {norm:.3}")
    // The loss is a negative mean log-likelihood plus the penalty, so its exponential
    // is roughly the inverse of the per-sample probability the model assigns.
    println("perplexity {run.last.exp():.3}")
    println("correct {run.correct} of 6")
    return run.correct
}
