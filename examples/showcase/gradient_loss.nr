// Showcase: training a least-squares fit by gradient descent.
//
// This is an integration example: it exercises several features *together*
// rather than in isolation:
//   Phase 3's `@grad` with `.backward()`, `.grad()` and `.zero_grad()`, a whole
//   training step · 2C's `@` contraction and broadcast arithmetic, which are the loss
//   itself, and compound assignment, which is the update · 2D's `pool` arena around
//   each step · a struct and a `Vec` recording the progress · string interpolation
//   with the format mini-language.
//
// `@grad` leaves `squared_error` exactly as written and adds a second function the
// compiler derives from it. `loss.backward()` runs that derivative and parks one
// gradient beside each parameter, where `.grad()` reads it. Nothing is recorded while
// the program runs: the derivative is ordinary compiled code, built before the
// program starts.

// Every tensor parameter is differentiated, so each one is borrowed `&mut`: the
// gradient belongs to the caller's tensor, not to a copy. `scale` is a plain
// number and is treated as a constant.
@grad
func squared_error(w: &mut Tensor<f32, [2, 1]>, b: &mut Tensor<f32, [1]>, scale: f32) -> Tensor<f32, []> {
    // Four samples of two features, and the targets they should map to. The targets
    // are exactly `x @ [1, 2] + 1`, so the loss reaches zero at that weight.
    val x: Tensor<f32, [4, 2]> = [[1.0, 2.0], [2.0, 0.0], [0.0, 1.0], [3.0, 1.0]]
    val y: Tensor<f32, [4, 1]> = [[6.0], [3.0], [3.0], [6.0]]
    // `[4, 2] @ [2, 1]` is one prediction per sample; the `[1]` bias is stretched
    // down all four.
    val prediction = x @ w
    val shifted = prediction + b
    val residual = shifted - y
    val squares = &residual * &residual
    return Tensor::scalar(squares.mean() * scale)
}

// `Copy`, so a `Vec` may hold it.
@derive(Copy, Clone)
struct Checkpoint {
    step: i32,
    loss: f32
}

const RATE: f32 = 0.05
const STEPS: i32 = 200
const EVERY: i32 = 40

func main() -> i32 {
    mut w: Tensor<f32, [2, 1]> = [[0.0], [0.0]]
    mut b: Tensor<f32, [1]> = [0.0]
    mut progress: Vec<Checkpoint> = Vec::new()
    for step in 0..STEPS {
        mut measured = 0.0f32
        // Everything a step computes lives in the arena and goes at the closing brace.
        // The gradients do not: they belong to `w` and `b`, which outlive the block.
        pool {
            val loss = squared_error(&mut w, &mut b, 1.0f32)
            // From the call to this line `w` and `b` stay mutably borrowed, because
            // this is where their gradients are written.
            loss.backward()
            w -= RATE * w.grad()
            b -= RATE * b.grad()
            w.zero_grad()
            b.zero_grad()
            measured = loss.sum()
        }
        if step % EVERY == 0 {
            progress.push(Checkpoint { step: step, loss: measured })
        }
    }

    mut improved = 0
    for i in 0..progress.len() as i32 {
        val checkpoint = progress[i]
        println("step {checkpoint.step:>3}: loss {checkpoint.loss:>7.4}")
        if i > 0 && checkpoint.loss < progress[i - 1].loss {
            improved += 1
        }
    }
    // The data are exactly `x @ [1, 2] + 1`, so the weights head there.
    println("w = [{w[0, 0]:.2}, {w[1, 0]:.2}], b = {b[0]:.2}")
    return improved * 10 + progress.len() as i32
}
