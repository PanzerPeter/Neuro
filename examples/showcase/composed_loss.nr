// Showcase: a `@grad` loss assembled from ordinary helper functions.
//
// This is an integration example: it exercises several features *together*
// rather than in isolation:
//   Phase 3's `@grad` through calls: the loss is built from helpers, one of them
//   shape-generic and one of them branching · 1F's generics, inferred from a
//   tensor argument's shape · 2C's `@` contraction · 2D's `pool` arena around each
//   evaluation · a struct and a `Vec` collecting the results · string
//   interpolation with the format mini-language.
//
// The compiler derives the gradient of each `@grad` function beside it at compile
// time, and follows every call into the callee's body to do so: the helpers need no
// annotation of their own, and the rest of the program calls them as usual.
//
// The program cannot read that gradient yet (`.backward()` and `.grad()` are the
// next layer), so what runs here is the primal. The compiler's own tests check the
// derivative through calls against finite differences.

func squared_distance(prediction: &Tensor<f32, [3, 1]>, target: &Tensor<f32, [3, 1]>) -> f32 {
    val residual = prediction - target
    val squares = &residual * &residual
    return squares.sum()
}

@grad
func fit(w: &mut Tensor<f32, [2, 1]>) -> Tensor<f32, []> {
    val x: Tensor<f32, [3, 2]> = [[1.0, 2.0], [2.0, 0.0], [0.0, 1.0]]
    val y: Tensor<f32, [3, 1]> = [[5.0], [2.0], [2.0]]
    val prediction = x @ w
    return Tensor::scalar(squared_distance(&prediction, &y))
}

// Generic over the row count, so it serves any column of weights. It charges only
// weights whose squared size passes `budget`, which makes it a branch the derivative
// follows the same way the call does.
func penalty<N>(w: &mut Tensor<f32, [N, 1]>, budget: f32) -> f32 {
    val first = w[0, 0]
    val second = w[1, 0]
    val size = first * first + second * second
    if size > budget { return size - budget }
    return 0.0
}

@grad
func regularized_fit(w: &mut Tensor<f32, [2, 1]>, budget: f32, strength: f32) -> Tensor<f32, []> {
    val x: Tensor<f32, [3, 2]> = [[1.0, 2.0], [2.0, 0.0], [0.0, 1.0]]
    val y: Tensor<f32, [3, 1]> = [[5.0], [2.0], [2.0]]
    val prediction = x @ w
    val error = squared_distance(&prediction, &y)
    return Tensor::scalar(error + penalty(w, budget) * strength)
}

@derive(Copy, Clone)
struct Scored {
    plain: f32,
    regularized: f32
}

func main() -> i32 {
    // The exact fit is `[1, 2]`, whose squared size 5 passes the budget of 4.
    mut results: Vec<Scored> = Vec::new()
    for step in 0..3 {
        mut plain = 0.0f32
        mut regularized = 0.0f32
        pool {
            val shrink = step as f32 * 0.5f32
            mut w: Tensor<f32, [2, 1]> = Tensor::from([[1.0f32], [2.0f32 - shrink]])
            plain = fit(&mut w).sum()
            regularized = regularized_fit(&mut w, 4.0f32, 2.0f32).sum()
        }
        results.push(Scored { plain: plain, regularized: regularized })
    }

    mut charged = 0
    for i in 0..3 {
        val scored = results[i]
        println("candidate {i}: fit {scored.plain:>6.3}, regularized {scored.regularized:>6.3}")
        if scored.regularized > scored.plain {
            charged += 1
        }
    }
    println("{charged} of {results.len()} candidates over budget")
    return charged * 10 + results.len() as i32
}
