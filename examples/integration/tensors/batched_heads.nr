// Two attention heads computed as one batch. Integration example: batched `@` with a
// projection shared across the batch, `einsum` with `...` over a shape parameter, a struct
// holding the per-head tensors, `.map` on the scores, and a `@grad` function whose shared
// weight collects its gradient from both heads. The heads are the leading axis of every
// tensor, so no loop over heads appears anywhere. The exit code is the projection
// gradient's sum.

struct Heads {
    queries: Tensor<f32, [2, 3, 4]>,
    keys: Tensor<f32, [2, 3, 4]>,
    values: Tensor<f32, [2, 3, 4]>
}

// Query-key scores for any number of leading axes: one head, a stack of heads, or a batch
// of stacks. `...` stands for whatever `S` binds.
func scores<S>(q: &Tensor<f32, [..S, 3, 4]>, k: &Tensor<f32, [..S, 3, 4]>) -> Tensor<f32, [..S, 3, 3]> {
    einsum("...qd,...kd->...qk", q, k)
}

// Mixing values by their weights is a batched product, written once over the batch rank.
func mix<S, L, D>(weights: &Tensor<f32, [..S, L, L]>, values: &Tensor<f32, [..S, L, D]>) -> Tensor<f32, [..S, L, D]> {
    weights @ values
}

// A projection of shape [4, 4] is applied to both heads at once: the rank-2 right operand
// repeats across the head axis, so its gradient sums both heads' contributions.
@grad(wrt: [projection])
func readout(projection: &mut Tensor<f32, [4, 4]>, mixed: &Tensor<f32, [2, 3, 4]>) -> Tensor<f32, []> {
    val out = mixed @ projection
    Tensor::scalar(out.sum())
}

func main() -> i32 {
    val heads = Heads {
        queries: [
            [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]],
            [[1.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 1.0], [1.0, 0.0, 1.0, 0.0]]
        ],
        keys: [
            [[1.0, 0.0, 0.0, 0.0], [1.0, 1.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0]],
            [[0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [1.0, 1.0, 1.0, 1.0]]
        ],
        values: [
            [[1.0, 2.0, 0.0, 0.0], [0.0, 1.0, 1.0, 0.0], [0.0, 0.0, 1.0, 2.0]],
            [[2.0, 0.0, 0.0, 1.0], [0.0, 2.0, 1.0, 0.0], [1.0, 1.0, 1.0, 1.0]]
        ]
    }

    // [2, 3, 4] against [2, 3, 4], contracted over the feature letter per head.
    val raw = scores(&heads.queries, &heads.keys)
    println("head 0 scores row 1: [{raw[0, 1, 0]}, {raw[0, 1, 1]}, {raw[0, 1, 2]}]")
    println("head 1 scores row 2: [{raw[1, 2, 0]}, {raw[1, 2, 1]}, {raw[1, 2, 2]}]")

    // One head alone is the same function with `S` bound to no axes.
    val first_q = Tensor::<f32, [3, 4]>::ones()
    val first_k = Tensor::<f32, [3, 4]>::ones()
    println("one head of ones scores {scores(&first_q, &first_k).sum()} in total")

    // Halve every score, then mix the values with them, head by head.
    val weights = raw.map(|s: f32| -> f32 { s * 0.5 })
    val mixed = mix(&weights, &heads.values)
    println("mixed head 1 row 2: [{mixed[1, 2, 0]}, {mixed[1, 2, 1]}, {mixed[1, 2, 2]}, {mixed[1, 2, 3]}]")

    mut projection = Tensor::<f32, [4, 4]>::identity()
    val total = readout(&mut projection, &mixed)
    total.backward()
    val gradient = projection.grad()
    println("readout {total.item()}, gradient row 0 sums both heads: {gradient[0, 0]}")
    gradient.sum() as i32
}
