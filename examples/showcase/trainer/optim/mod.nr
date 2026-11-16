// Module `optim`: a directory module whose child `schedule` it re-exports, so a caller
// reaches `optim::decay` without knowing the child exists.

export import ./schedule::{decay}

import model::{Linear}

// `rate` is private. Another module cannot write `Sgd { rate: 5.0 }`, so the only way to
// hold an `Sgd` is `Sgd::new`, which refuses a rate that would make training diverge.
export struct Sgd {
    rate: f32
}

impl Sgd {
    func new(rate: f32) -> Option<Sgd> {
        if rate > 0.0f32 && rate < 1.0f32 {
            return Some(Sgd { rate: rate })
        }
        None
    }

    // Move each parameter against the gradient the last `.backward()` left on it, at the
    // rate the schedule gives for this step, then clear the gradients for the next one.
    func apply(&self, model: &mut Linear, step: i32) {
        val rate = decay(self.rate, step)
        model.w -= rate * model.w.grad()
        model.b -= rate * model.b.grad()
        model.w.zero_grad()
        model.b.zero_grad()
    }
}
