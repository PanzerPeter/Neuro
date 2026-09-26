// Showcase: fine-tuning a model's head while its feature layer stays frozen.
//
// This is an integration example: it exercises several features *together*
// rather than in isolation:
//   Phase 3's `@grad(wrt: [...])`, whose list names the head's two tensors through the
//   receiver's exported fields, so `.backward()` fills only their gradient slots and the
//   feature layer is a constant of the derivative · the `@` contraction and broadcast
//   arithmetic the loss is made of · 2D's `pool` arena around each step · structs nested
//   in structs, `&mut self` methods, and a `Vec` of `Copy` records · string interpolation
//   with the format mini-language.
//
// The features are "pretrained": they are fixed, and only the head learns to read them.
// Three fine-tuning budgets are compared, each on a fresh copy of the same model.

struct Dense {
    export w: Tensor<f32, [2, 1]>,
    export b: Tensor<f32, [1]>
}

struct Model {
    export features: Tensor<f32, [2, 2]>,
    export head: Dense
}

@derive(Copy, Clone)
struct Budget {
    steps: i32,
    loss: f32
}

impl Model {
    // `wrt:` picks the head; `features` is read but never differentiated. A tensor field
    // cannot be borrowed yet, so the body works on copies of the fields it multiplies.
    @grad(wrt: [self.head.w, self.head.b])
    func loss(&mut self, x: &Tensor<f32, [4, 2]>, y: &Tensor<f32, [4, 1]>) -> Tensor<f32, []> {
        val features = self.features.clone()
        val hidden = x @ features
        val w = self.head.w.clone()
        val out = hidden @ w
        val shifted = out + self.head.b.clone()
        val residual = shifted - y
        val squares = &residual * &residual
        return Tensor::scalar(squares.mean())
    }

    // The receiver stays mutably borrowed from each `@grad` call to its `.backward()`.
    func fine_tune(&mut self, steps: i32, x: &Tensor<f32, [4, 2]>, y: &Tensor<f32, [4, 1]>) -> f32 {
        mut last = 0.0f32
        mut step = 0
        while step < steps {
            pool {
                val l = self.loss(x, y)
                l.backward()
                self.head.w -= 0.05f32 * self.head.w.grad()
                self.head.b -= 0.05f32 * self.head.b.grad()
                self.head.w.zero_grad()
                self.head.b.zero_grad()
                last = l.sum()
            }
            step += 1
        }
        return last
    }
}

func pretrained() -> Model {
    return Model {
        features: [[1.0, 0.5], [-0.5, 1.0]],
        head: Dense { w: [[0.0], [0.0]], b: [0.0] }
    }
}

func main() -> i32 {
    val x: Tensor<f32, [4, 2]> = [[1.0, 0.0], [0.0, 1.0], [1.0, 1.0], [2.0, -1.0]]
    // Exactly `(x @ features) @ [2, -1] + 0.5`, so a perfect head exists.
    val y: Tensor<f32, [4, 1]> = [[2.0], [-1.5], [0.0], [5.5]]

    mut budgets: Vec<Budget> = Vec::new()
    val steps: [i32; 3] = [10, 40, 400]
    for i in 0..3 {
        mut model = pretrained()
        val loss = model.fine_tune(steps[i], &x, &y)
        // The feature layer came through fine-tuning untouched.
        assert(model.features[0, 1] == 0.5f32 && model.features[1, 0] == -0.5f32)
        budgets.push(Budget { steps: steps[i], loss: loss })
        if i == 2 {
            println("head after {steps[i]} steps: w = [{model.head.w[0, 0]:.2}, {model.head.w[1, 0]:.2}], b = {model.head.b[0]:.2}")
        }
    }

    mut improving = 0
    for i in 0..3 {
        val budget = budgets[i]
        println("{budget.steps:>4} steps: loss {budget.loss:>8.5}")
        if i > 0 && budget.loss < budgets[i - 1].loss {
            improving += 1
        }
    }
    assert(budgets[2].loss < 0.001f32)
    println("{improving} of {budgets.len() - 1} longer budgets lowered the loss")
    return improving * 10 + budgets.len() as i32
}
