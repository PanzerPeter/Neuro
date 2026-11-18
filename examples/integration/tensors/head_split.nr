// Multi-head attention scores written once over every size. Integration example: a struct
// sized by bare names whose generic methods split a projection into heads with
// `.reshape([B, L, H, D / H])`, a batched `@` against a shared [D, D] weight, an `einsum`
// whose letters bind the symbolic head width, a per-head scale reshaped to broadcast, and a
// reduction along the last axis. Each head count is its own instance, checked when it is
// instantiated. The exit code is the sum of each query's best score with two heads.

struct Attention<D, H> {
    wq: Tensor<f32, [D, D]>,
    wk: Tensor<f32, [D, D]>,
    scale: Tensor<f32, [H]>
}

impl<D, H> Attention<D, H> {
    // One projection, cut into `H` heads of `D / H` features each.
    func project<B, L>(&self, x: &Tensor<f32, [B, L, D]>, w: &Tensor<f32, [D, D]>) -> Tensor<f32, [B, L, H, D / H]> {
        (x @ w).reshape([B, L, H, D / H])
    }

    // Query-key scores per head, each head scaled by its own factor.
    func scores<B, L>(&self, x: &Tensor<f32, [B, L, D]>) -> Tensor<f32, [B, H, L, L]> {
        val q = self.project(x, &self.wq)
        val k = self.project(x, &self.wk)
        val raw = einsum("blhe,bmhe->bhlm", &q, &k)
        raw * self.scale.clone().reshape([1, H, 1, 1])
    }

    // The best key for every query of every head, summed.
    func best<B, L>(&self, x: &Tensor<f32, [B, L, D]>) -> f32 {
        self.scores(x).max(axis: -1).sum()
    }
}

func identity() -> Tensor<f32, [4, 4]> {
    [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]
}

func main() -> i32 {
    val x: Tensor<f32, [1, 3, 4]> = [[[1.0, 0.0, 0.0, 1.0], [0.0, 1.0, 1.0, 0.0], [1.0, 1.0, 0.0, 0.0]]]

    // Two heads of two features: the first head's best scores are 2, 2 and 4 at scale 2,
    // the second's 1, 1 and 0.
    val two: Attention<4, 2> = Attention { wq: identity(), wk: identity(), scale: [2.0, 1.0] }
    val pair = two.best(&x)
    println("two heads: {pair}")

    // One head of all four features at scale 3: every query's best score is 6.
    val one: Attention<4, 1> = Attention { wq: identity(), wk: identity(), scale: [3.0] }
    println("one head:  {one.best(&x)}")
    println("scores of the two heads: {two.scores(&x).sum()}")

    return pair as i32
}
