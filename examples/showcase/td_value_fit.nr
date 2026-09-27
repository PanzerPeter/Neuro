// Showcase: temporal-difference learning, where the bootstrapped target must not learn.
//
// This is an integration example: it exercises several features *together*
// rather than in isolation:
//   Phase 3's gradient fences, `.detach()` on a value and a call to a `@no_grad` function,
//   inside `@grad` losses built from the `@` contraction and broadcast arithmetic · 2D's
//   `pool` arena around each training step · a `Vec` of `Copy` records · string
//   interpolation with the format mini-language.
//
// A linear value function v(s) = bias + slope * s / 3 is fitted to a four-state chain that
// pays 1 on its last step. The TD target r + 0.9 v(s') is built from the same weights it
// trains, and semi-gradient TD treats it as a constant: fencing it with `.detach()` or
// computing it in a `@no_grad` function gives the same derivative, bit for bit.
// Differentiating through it instead (the residual gradient) minimizes a different error
// and settles on a steeper line.

const GAMMA: f32 = 0.9
const STEPS: i32 = 300
const RATE: f32 = 0.5

@derive(Copy, Clone)
struct Fit {
    bias: f32,
    slope: f32,
    residual: f32
}

// One row per state: a bias feature and the state's position along the chain.
func features() -> Tensor<f32, [4, 2]> {
    return [[1.0, 0.0], [1.0, 0.33333334], [1.0, 0.6666667], [1.0, 1.0]]
}

// The features of the state each one steps to. The last steps off the chain, to nothing.
func successors() -> Tensor<f32, [4, 2]> {
    return [[1.0, 0.33333334], [1.0, 0.6666667], [1.0, 1.0], [0.0, 0.0]]
}

func rewards() -> Tensor<f32, [4, 1]> {
    return [[0.0], [0.0], [0.0], [1.0]]
}

// The bootstrapped target, in a function no derivative looks into.
@no_grad
func bootstrap(w: &Tensor<f32, [2, 1]>) -> Tensor<f32, [4, 1]> {
    return rewards() + (successors() @ w) * GAMMA
}

// How far the value function is from satisfying its own Bellman equation. Outside a
// derivative, a `@no_grad` function is an ordinary function.
@no_grad
func bellman_residual(w: &Tensor<f32, [2, 1]>) -> f32 {
    val error = features() @ w - bootstrap(w)
    val squares = &error * &error
    return squares.mean()
}

@grad
func td_detached(w: &mut Tensor<f32, [2, 1]>) -> Tensor<f32, []> {
    val target = rewards() + (successors() @ w) * GAMMA
    val fixed = target.detach()
    val error = features() @ w - fixed
    val squares = &error * &error
    return Tensor::scalar(squares.mean())
}

@grad
func td_no_grad(w: &mut Tensor<f32, [2, 1]>) -> Tensor<f32, []> {
    val error = features() @ w - bootstrap(w)
    val squares = &error * &error
    return Tensor::scalar(squares.mean())
}

@grad
func residual_gradient(w: &mut Tensor<f32, [2, 1]>) -> Tensor<f32, []> {
    val target = rewards() + (successors() @ w) * GAMMA
    val error = features() @ w - target
    val squares = &error * &error
    return Tensor::scalar(squares.mean())
}

// Gradient descent from zero weights on the loss `kind` names.
func train(kind: i32) -> Fit {
    mut w: Tensor<f32, [2, 1]> = [[0.0], [0.0]]
    mut step = 0
    while step < STEPS {
        pool {
            if kind == 0 {
                val l = td_detached(&mut w)
                l.backward()
            } else if kind == 1 {
                val l = td_no_grad(&mut w)
                l.backward()
            } else {
                val l = residual_gradient(&mut w)
                l.backward()
            }
            w -= RATE * w.grad()
            w.zero_grad()
        }
        step += 1
    }
    return Fit { bias: w[0, 0], slope: w[1, 0], residual: bellman_residual(&w) }
}

func main() -> i32 {
    mut fits: Vec<Fit> = Vec::new()
    for kind in 0..3 {
        fits.push(train(kind))
    }
    val detached = fits[0]
    val frozen = fits[1]
    val unfenced = fits[2]
    // Both fences give semi-gradient TD, to the last bit.
    assert(detached.bias == frozen.bias && detached.slope == frozen.slope)
    println("TD, .detach():      v(s) = {detached.bias:.3} + {detached.slope:.3} s/3, residual {detached.residual:.7}")
    println("TD, @no_grad:       v(s) = {frozen.bias:.3} + {frozen.slope:.3} s/3, residual {frozen.residual:.7}")
    println("residual gradient:  v(s) = {unfenced.bias:.3} + {unfenced.slope:.3} s/3, residual {unfenced.residual:.7}")
    // The unfenced loss minimizes the residual itself, so it ends lower, on a steeper line.
    assert(unfenced.residual < detached.residual && unfenced.slope > detached.slope)
    return (unfenced.slope * 1000.0) as i32 - (detached.slope * 1000.0) as i32
}
