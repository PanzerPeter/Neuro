// Showcase: a least-squares loss marked for differentiation.
//
// This is an integration example: it exercises several features *together*
// rather than in isolation:
//   Phase 3's `@grad`, which compiles a derivative of `squared_error` beside it · 2C's
//   `@` contraction and broadcast arithmetic, which are the loss itself · 2D's
//   `pool` arena around each evaluation · tensor construction from computed
//   elements · a struct and a `Vec` collecting the trials · string interpolation
//   with the format mini-language.
//
// `@grad` leaves `squared_error` exactly as written and adds a second function the
// compiler derives from it: the same parameters, returning the loss together with
// one gradient per tensor parameter, shaped like that parameter. Nothing about it
// happens at run time. The derivative is ordinary compiled code, built before the
// program starts, which is why no graph is recorded during the calls below.
//
// The program cannot read that gradient yet: `.backward()` and `.grad()` are the
// next layer. What runs here is the primal, the part of a training step that
// measures how far each candidate is from the data. Its numerical twin, the
// derivative, is checked against finite differences by the compiler's own tests.

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
struct Trial {
    step: i32,
    loss: f32
}

func main() -> i32 {
    mut trials: Vec<Trial> = Vec::new()
    for step in 0..3 {
        // Each candidate walks halfway toward the answer: step 2 IS the answer.
        val t = step as f32 * 0.5f32
        // The candidate tensors live and die in the arena; only the measured number,
        // a plain `f32`, leaves the block.
        mut measured = 0.0f32
        pool {
            mut w: Tensor<f32, [2, 1]> = Tensor::from([[t], [t * 2.0f32]])
            mut b: Tensor<f32, [1]> = Tensor::from([t])
            val loss = squared_error(&mut w, &mut b, 1.0f32)
            measured = loss.sum()
        }
        trials.push(Trial { step: step, loss: measured })
    }

    mut best = trials[0]
    for i in 0..3 {
        val trial = trials[i]
        println("step {trial.step}: loss {trial.loss:>6.3}")
        if trial.loss < best.loss {
            best = trial
        }
    }
    // Every value above is exact in f32, so the equality is an equality.
    assert(best.loss == 0.0f32)
    println("best candidate: step {best.step}")
    return best.step * 10 + trials.len() as i32
}
