// Showcase: a small training project split into modules, each with one job. `data` holds
// the batches, `model` the layer and its `@grad` loss, `optim` an optimizer with a private
// rate and a child module `optim::schedule`, `train` the loop. A tensor's shape crosses
// every module boundary inside the type: `Batch` and `Linear::loss` both say `[6, 2]`, so a
// batch of the wrong size is a compile error where it is passed, not a crash later.
// `optim::Sgd` keeps its rate private, so the one way to build it is `Sgd::new`, which
// checks the rate.
//
//   showcase/trainer/
//     main.nr              <- this file, the root module
//     data.nr              <- module `data`
//     model.nr             <- module `model`
//     train.nr             <- module `train`
//     optim/
//       mod.nr             <- module `optim`, re-exporting `decay`
//       schedule.nr        <- module `optim::schedule`

import data::{training_set, holdout}
import model::{Linear}
import optim::{Sgd}
import train::{fit}

func main() -> i32 {
    // A rate outside (0, 1) never becomes an optimizer: the invariant lives in `optim`.
    val rejected = Sgd::new(-0.5f32)
    val Option::Some(opt) = Sgd::new(0.1f32) else {
        println("the learning rate was refused")
        return 1
    }
    assert(rejected.is_none())

    val train_set = training_set()
    val test_set = holdout()

    mut model = Linear::zeros()
    val before = model.error(&test_set.x, &test_set.y)
    val loss = fit(&mut model, &opt, &train_set, 100)
    val after = model.error(&test_set.x, &test_set.y)

    println("holdout error before training: {before:.3}")
    println("train loss after 100 steps   : {loss:.4}")
    println("holdout error after training : {after:.4}")
    println("w = [{model.w[0, 0]:.2}, {model.w[1, 0]:.2}], b = {model.b[0]:.2}")
    println("step size at step 0 and 200  : {optim::decay(0.1f32, 0):.3}, {optim::decay(0.1f32, 200):.3}")

    // The data are exactly `2 * x0 - x1 + 0.5`, so the weights head there.
    assert(after < before)
    assert(after < 0.1f32)
    return (model.w[0, 0] * 10.0f32 + 0.5f32) as i32
}
