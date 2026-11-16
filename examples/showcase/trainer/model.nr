// Module `model`: one linear layer. Its parameters are exported fields, so the optimizer
// module can read their gradients, and `loss` is `@grad`, so `.backward()` fills them.

export struct Linear {
    export w: Tensor<f32, [2, 1]>,
    export b: Tensor<f32, [1]>
}

impl Linear {
    func zeros() -> Linear {
        Linear { w: [[0.0], [0.0]], b: [0.0] }
    }

    // `wrt:` names the two parameters. The batch is read, never differentiated, and its
    // six samples are in the type, so a batch of any other size does not compile here.
    @grad(wrt: [self.w, self.b])
    func loss(&mut self, x: &Tensor<f32, [6, 2]>, y: &Tensor<f32, [6, 1]>) -> Tensor<f32, []> {
        val w = self.w.clone()
        val out = x @ w + self.b.clone()
        val residual = out - y
        val squares = &residual * &residual
        return Tensor::scalar(squares.mean())
    }

    // The same error with no gradient, for samples the model is only scored on.
    func error(&self, x: &Tensor<f32, [6, 2]>, y: &Tensor<f32, [6, 1]>) -> f32 {
        val w = self.w.clone()
        val out = x @ w + self.b.clone()
        val residual = out - y
        val squares = &residual * &residual
        squares.mean()
    }
}
