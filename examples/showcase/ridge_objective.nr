// Showcase: a training objective carried as a value, with its loss a `@grad` method.
// Integration example: `@grad` on a method whose receiver is a constant the loss reads,
// `.backward()`, `.grad()` and `.zero_grad()` driving gradient descent, `@` and
// broadcasting, a `pool` around each step, nested structs, and a `Vec` of `Copy` records.
// Each `Objective` carries its own ridge penalty. The derivative treats the receiver as a
// constant, so its fields steer the gradient without receiving one. A stronger penalty
// pulls the weights harder toward zero.

struct Penalty {
    strength: f32
}

struct Objective {
    penalty: Penalty,
    steps: i32
}

@derive(Copy, Clone)
struct Run {
    strength: f32,
    loss: f32,
    weight: f32
}

impl Objective {
    // Every tensor parameter is differentiated, so both are borrowed `&mut`. The
    // receiver is only read, so `&self` is enough.
    @grad
    func loss(&self, w: &mut Tensor<f32, [2, 1]>, b: &mut Tensor<f32, [1]>) -> Tensor<f32, []> {
        // Four samples whose targets are exactly `x @ [1, 2] + 1`.
        val x: Tensor<f32, [4, 2]> = [[1.0, 2.0], [2.0, 0.0], [0.0, 1.0], [3.0, 1.0]]
        val y: Tensor<f32, [4, 1]> = [[6.0], [3.0], [3.0], [6.0]]
        val prediction = x @ w
        val shifted = prediction + b
        val residual = shifted - y
        val squares = &residual * &residual
        val ridge = w * w
        return Tensor::scalar(squares.mean() + ridge.sum() * self.penalty.strength)
    }

    // An ordinary method calling the `@grad` one on its own receiver.
    func train(&self) -> Run {
        mut w: Tensor<f32, [2, 1]> = [[0.0], [0.0]]
        mut b: Tensor<f32, [1]> = [0.0]
        mut measured = 0.0f32
        mut step = 0
        while step < self.steps {
            pool {
                val loss = self.loss(&mut w, &mut b)
                // `w` and `b` stay mutably borrowed from the call to this line.
                loss.backward()
                w -= 0.05f32 * w.grad()
                b -= 0.05f32 * b.grad()
                w.zero_grad()
                b.zero_grad()
                measured = loss.sum()
            }
            step += 1
        }
        return Run { strength: self.penalty.strength, loss: measured, weight: w[1, 0] }
    }
}

func main() -> i32 {
    mut runs: Vec<Run> = Vec::new()
    val strengths: [f32; 3] = [0.0, 0.1, 1.0]
    for i in 0..3 {
        val objective = Objective { penalty: Penalty { strength: strengths[i] }, steps: 400 }
        runs.push(objective.train())
    }

    mut shrinking = 0
    for i in 0..3 {
        val run = runs[i]
        println("penalty {run.strength:>4.1}: loss {run.loss:>6.3}, second weight {run.weight:>6.3}")
        if i > 0 && run.weight < runs[i - 1].weight {
            shrinking += 1
        }
    }
    // With no penalty the fit is exact, and the second weight reaches its true value.
    assert(runs[0].loss < 0.001f32)
    println("{shrinking} of {runs.len() - 1} stronger penalties shrank the weight")
    return shrinking * 10 + runs.len() as i32
}
