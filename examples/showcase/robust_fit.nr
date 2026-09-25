// Showcase: a loss with branches and a loop, marked for differentiation.
//
// This is an integration example: it exercises several features *together*
// rather than in isolation:
//   Phase 3's `@grad` through control flow: a `while` that damps the prediction,
//   an `if` expression that soft-clips the error, and the `mut` bindings both
//   reassign · 2C's `@` contraction and broadcast arithmetic · 2D's `pool` arena
//   around each evaluation · a struct and a `Vec` collecting the trials · string
//   interpolation with the format mini-language.
//
// The compiler derives `robust_fit`'s gradient beside it at compile time. Where the
// body branches, the derivative follows the branch the call actually takes, and
// where it loops, the derivative undoes exactly the iterations the call ran. Nothing
// is recorded while the program runs: the backward pass recomputes what it needs.
//
// This program only evaluates the losses, so no derivative runs: a `@grad` call
// runs its derivative only for a `.backward()`, as `gradient_loss.nr` does. The
// compiler's own tests check the derivative of loops and branches like these
// against finite differences.

// Every tensor parameter is differentiated, so `w` is borrowed `&mut`. `limit` and
// `sweeps` are plain numbers, constants as far as the derivative is concerned.
@grad
func robust_fit(w: &mut Tensor<f32, [2, 1]>, limit: f32, sweeps: i32) -> Tensor<f32, []> {
    // Three samples whose targets are exactly `x @ [1, 2]`.
    val x: Tensor<f32, [3, 2]> = [[1.0, 2.0], [2.0, 0.0], [0.0, 1.0]]
    val y: Tensor<f32, [3, 1]> = [[5.0], [2.0], [2.0]]
    mut prediction = x @ w
    // Each sweep halves the prediction. The trip count is an argument, so the
    // derivative has to undo however many sweeps this call made.
    mut done = 0
    while done < sweeps {
        prediction = &prediction * 0.5
        done += 1
    }
    val residual = prediction - y
    val squares = &residual * &residual
    val error = squares.mean()
    // Quadratic up to `limit`, a tenth as steep past it. Both arms meet at `limit`,
    // and exactly there the derivative is the one of the arm the call takes.
    val clipped = if error <= limit { error } else { limit + (error - limit) * 0.1 }
    return Tensor::scalar(clipped)
}

@derive(Copy, Clone)
struct Trial {
    sweeps: i32,
    loss: f32
}

func main() -> i32 {
    mut trials: Vec<Trial> = Vec::new()
    for sweeps in 0..3 {
        mut measured = 0.0f32
        pool {
            mut w: Tensor<f32, [2, 1]> = Tensor::from([[1.0f32], [2.0f32]])
            val loss = robust_fit(&mut w, 1.0f32, sweeps)
            measured = loss.sum()
        }
        trials.push(Trial { sweeps: sweeps, loss: measured })
    }

    mut clipped = 0
    for i in 0..3 {
        val trial = trials[i]
        println("sweeps {trial.sweeps}: loss {trial.loss:>6.3}")
        if trial.loss > 1.0f32 {
            clipped += 1
        }
    }
    // Zero sweeps is the exact fit; every damped trial lands past the limit.
    assert(trials[0].loss == 0.0f32)
    println("{clipped} of {trials.len()} trials clipped")
    return clipped * 10 + trials.len() as i32
}
