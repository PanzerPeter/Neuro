// Showcase: mixed-precision updates, with `bf16` weights and an `f32` master copy.
//
// This is an integration example: it exercises several features *together*
// rather than in isolation:
//   half-precision scalar arithmetic, rounded once per operation · a `@derive(Copy)`
//   struct of `bf16` fields, updated by a function and collected in a `Vec` · `for` loops
//   over ranges · a closure through `.map` on a `bf16` tensor · `.topk` ordering `bf16`
//   scores · `.to_checked` with `match` · interpolation of widened values.
//
// A `bf16` weight of 1.0 cannot take an update of 2^-9: the step is a quarter of the gap
// between 1.0 and the next `bf16` value, so every `weight - lr * grad` rounds back to 1.0
// and a thousand steps change nothing. Keeping an `f32` master copy and narrowing it
// once at the end is the usual fix, and it moves the weight by the whole thousand steps.
// Both are exact per-operation results, the same on every machine.

@derive(Copy, Clone)
struct Param {
    weight: bf16,
    grad: bf16
}

func step(p: Param, lr: bf16) -> Param {
    Param { weight: p.weight - lr * p.grad, grad: p.grad }
}

func main() -> i32 {
    val lr: bf16 = 0.001953125
    mut stalled = Param { weight: 1.0, grad: -1.0 }
    mut master: f32 = 1.0
    for i in 0..1000 {
        stalled = step(stalled, lr)
        master -= lr as f32 * stalled.grad as f32
    }
    val moved = master as bf16
    println("bf16 weight after 1000 steps   = {stalled.weight as f32}")
    println("f32 master, narrowed once      = {moved as f32}")

    // A layer's worth of parameters, each stepped once in bf16. Only an update past half
    // the gap above 1.0 moves its weight; the smaller ones are lost to rounding, and the
    // one exactly at half rounds to the even neighbour, 1.0.
    mut params: Vec<Param> = Vec::new()
    for i in 0..6 {
        val grad = 0.0 - (i as bf16) * 0.03125
        params.push(step(Param { weight: 1.0, grad: grad }, 0.0625))
    }
    mut changed = 0
    for i in 0..6 {
        val p = params[i]
        if p.weight != 1.0 {
            changed += 1
        }
        println("param {i}: grad {p.grad as f32}, weight {p.weight as f32}")
    }

    // Scores scaled in bf16 and the best two picked out, ties to the earlier index.
    val scores: Tensor<bf16, [5]> = [0.25, 3.5, -1.0, 2.0, 0.75]
    val scaled = scores.map(|s: bf16| s * 0.5 + 0.125)
    val (best, at) = scaled.topk(k: 2)
    println("top scores [{best[0] as f32}, {best[1] as f32}] at [{at[0]}, {at[1]}]")

    // A weight as a percentage byte: `None` past 255 instead of a clamped 255.
    val percent = match (moved * 100.0).to_checked::<u8>() {
        Option::Some(code) => code as i32,
        Option::None => -1
    }
    println("master weight as a byte        = {percent}")

    if stalled.weight == 1.0 && moved > 2.0 && changed == 3 && at[0] == 1 && percent == -1 {
        return changed + at[1]
    }
    return 0
}
