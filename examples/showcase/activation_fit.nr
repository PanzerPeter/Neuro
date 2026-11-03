// Showcase: one neuron trained under two activation functions passed in as values.
// Integration example: the activation is a parameter of a `@grad` loss, applied with `.map`
// and summed with `.reduce`. `>>` composition and closures supply the two activations, `@`
// the forward pass, and `+=` the update. The derivative is built at compile time for each
// function value named at the call (a closure literal or a composition). A function chosen
// at run time is refused, since no derivative was built for it.

// Four samples of two features. The targets are exactly `x @ [1, 2]`.

@grad
func activated_error(w: &mut Tensor<f32, [2, 1]>, activate: (f32) -> f32) -> Tensor<f32, []> {
    val x: Tensor<f32, [4, 2]> = [[1.0, 2.0], [2.0, 0.0], [0.0, 1.0], [3.0, 1.0]]
    val y: Tensor<f32, [4, 1]> = [[5.0], [2.0], [2.0], [5.0]]
    val prediction = (x @ w).map(activate)
    val residual = prediction - y
    val squares = residual.map(|r: f32| -> f32 { r * r })
    return Tensor::scalar(squares.reduce(0.0f32, |acc: f32, s: f32| -> f32 { acc + s }) * 0.25)
}

func lift(v: f32) -> f32 {
    v + 0.5
}

// Pulls large values back toward zero, the way a saturating activation would.
func soften(v: f32) -> f32 {
    v - v * v * v * 0.01
}

@derive(Copy, Clone)
struct Trial {
    first: f32,
    last: f32
}

const RATE: f32 = 0.02
const STEPS: i32 = 60

func main() -> i32 {
    mut trials: Vec<Trial> = Vec::new()

    // The closure captures `gain` where it is written, and its derivative reads the
    // same value.
    val gain = 1.0f32
    mut scaled: Tensor<f32, [2, 1]> = [[0.0], [0.0]]
    mut trial = Trial { first: 0.0, last: 0.0 }
    for step in 0..STEPS {
        val loss = activated_error(&mut scaled, |v: f32| -> f32 { v * gain })
        loss.backward()
        scaled -= RATE * scaled.grad()
        scaled.zero_grad()
        trial.last = loss.sum()
        if step == 0 { trial.first = trial.last }
    }
    trials.push(trial)

    mut softened: Tensor<f32, [2, 1]> = [[0.0], [0.0]]
    trial = Trial { first: 0.0, last: 0.0 }
    for step in 0..STEPS {
        val loss = activated_error(&mut softened, lift >> soften)
        loss.backward()
        softened -= RATE * softened.grad()
        softened.zero_grad()
        trial.last = loss.sum()
        if step == 0 { trial.first = trial.last }
    }
    trials.push(trial)

    mut improved = 0
    for i in 0..trials.len() as i32 {
        val result = trials[i]
        println("activation {i}: loss {result.first:>7.4} -> {result.last:>7.4}")
        if result.last < result.first {
            improved += 1
        }
    }
    println("scaled   w = [{scaled[0, 0]:.2}, {scaled[1, 0]:.2}]")
    println("softened w = [{softened[0, 0]:.2}, {softened[1, 0]:.2}]")
    return improved * 10 + trials.len() as i32
}
