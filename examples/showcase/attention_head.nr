// Showcase: one scaled dot-product attention head, written as contractions.
//
// This is an integration example: it exercises several features *together*
// rather than in isolation:
//   Einstein notation (`einsum`) for both contractions · the pipeline operator
//   `|>` and the composition operator `>>` chaining the scaling stages · a struct
//   holding tensor fields, moved into it at construction · named tensor dimensions
//   on the annotations · reductions (`.sum()`, `.max()`) over a borrowed tensor ·
//   a borrowed receiver a method reads without moving · `for`-in over a range ·
//   string interpolation with the format mini-language.
//
// An attention head is two contractions with a rescale between them. The scores are
// every query row against every key row, which is `einsum("td,sd->ts", ...)`: the
// feature letter `d` is summed away and the two token letters survive. The context is
// every score row against the value rows, `einsum("ts,sf->tf", ...)`: the source-token
// letter `s` is summed away this time. Neither is a matrix product as written — the
// first contracts the LAST axis of both operands — so spelling them with `@` would mean
// transposing a buffer first, and spelling them with loops would mean four of them.
//
// The exit code is the rounded total of the context tensor.

struct Head {
    queries: Tensor<f32, [tokens: 3, feat: 4]>,
    keys: Tensor<f32, [tokens: 3, feat: 4]>,
    values: Tensor<f32, [tokens: 3, out: 2]>
}

// The two rescaling stages, as plain names so `>>` can compose them: a composed stage
// is a call to a name, never a captured value.
func by_sqrt_dim(score: f32) -> f32 {
    score / 2.0
}

func clamp_low(score: f32) -> f32 {
    if score < 0.0 { return 0.0 }
    return score
}

impl Head {
    // `&self` reads the two weight tensors; `einsum` reads them in turn, so nothing is
    // moved out of the head and it stays usable afterwards.
    func scores(&self) -> Tensor<f32, [3, 3]> {
        einsum("td,sd->ts", self.queries, self.keys)
    }

    func context(&self, weights: &Tensor<f32, [3, 3]>) -> Tensor<f32, [3, 2]> {
        einsum("ts,sf->tf", weights, self.values)
    }
}

func main() -> i32 {
    val head = Head {
        queries: [
            [1.0, 0.0, 1.0, 0.0],
            [0.0, 2.0, 0.0, 0.0],
            [1.0, 1.0, 0.0, 1.0]
        ],
        keys: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 1.0],
            [0.0, 0.0, 2.0, 0.0]
        ],
        values: [
            [1.0, 0.0],
            [0.0, 1.0],
            [2.0, 2.0]
        ]
    }

    // Contraction one: the feature axis is summed away and both token axes survive.
    val raw: Tensor<f32, [tokens: 3, source: 3]> = head.scores()
    for row in 0..3 {
        println("scores {row}   = {raw[row, 0]:.1}, {raw[row, 1]:.1}, {raw[row, 2]:.1}")
    }

    // `>>` builds the rescale once, as a value; `|>` applies it to each element read
    // back out. The composed function is a name the chain calls, not a closure it holds.
    val rescale = by_sqrt_dim >> clamp_low
    mut scaled: Tensor<f32, [3, 3]> = [
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0]
    ]
    for row in 0..3 {
        for col in 0..3 {
            scaled[row, col] = raw[row, col] |> rescale
        }
    }
    println("peak score  = {scaled.max():.2}")

    // Contraction two: the source-token axis is summed away, the output feature axis
    // arrives from the values.
    val context: Tensor<f32, [tokens: 3, out: 2]> = head.context(&scaled)
    for row in 0..3 {
        println("context {row}  = {context[row, 0]:.2}, {context[row, 1]:.2}")
    }

    // The head is still whole: both contractions read it.
    val budget: Tensor<f32, [3, 4]> = head.queries.clone()
    println("query mass  = {budget.sum():.1}")

    val total = context.sum()
    println("context sum = {total:.2}")
    return total as i32
}
