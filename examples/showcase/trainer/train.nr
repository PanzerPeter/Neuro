// Module `train`: the loop that joins the model, the optimizer and a batch. The shapes
// in `Batch` and in `Linear::loss` are the same, checked when this file is compiled.

import data::{Batch}
import model::{Linear}
import optim::{Sgd}

export func fit(model: &mut Linear, opt: &Sgd, batch: &Batch, steps: i32) -> f32 {
    val x = batch.x.clone()
    val y = batch.y.clone()
    mut last = 0.0f32
    mut step = 0
    while step < steps {
        // Everything a step computes lives in the arena. The gradients do not: they
        // belong to the model's tensors, which outlive the block.
        pool {
            val loss = model.loss(&x, &y)
            loss.backward()
            opt.apply(model, step)
            last = loss.item()
        }
        step += 1
    }
    last
}
