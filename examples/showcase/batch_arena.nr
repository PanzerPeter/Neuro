// Showcase: a batched forward pass run inside the pool allocator.
//
// This is an integration example: it exercises several features *together*
// rather than in isolation:
//   2D's `pool` block, taking its arena mark once per batch · 2B's tensor
//   literal coercion and construction helpers · 2C's `@` matrix multiplication
//   and its by-value element-wise operators with scalar and row broadcast ·
//   2B's reductions, which read a batch back out as one number · a struct with
//   a method on `&self` · a `for` loop over a range · string interpolation with
//   the format mini-language.
//
// The shape a training loop actually has: weights and running totals declared
// OUTSIDE the arena, so they survive it, and every per-batch intermediate built
// INSIDE it, so the batch's memory goes back in one store at the closing brace.
// The loop below allocates a product matrix, a scaled copy and a bias-broadcast
// sum per step, and its high-water mark never moves — which is the whole reason
// to reach for an arena over the heap in a loop like this.
//
// The two rules the block enforces are both visible here. `running` and `steps`
// are an `f32` and an `i32`, so the inner block may write them: a scalar carries
// no address into the arena. No tensor declared before the `pool` is written from
// inside it, and no `break` or `return` leaves the block — the loop's exit is the
// loop's own, one level out.

// A running tally, carried across the batches on the stack rather than in the
// arena. Its fields are scalars, so it never allocates at all.
struct Tally {
    steps: i32,
    total: f32
}

impl Tally {
    // `&self` reads the tally without consuming it, so the same value can be
    // rendered and then kept.
    func render(&self) -> string {
        return "{self.steps} batches, mean sum {self.total:.3}"
    }
}

func main() -> i32 {
    // The weights outlive every pool below: they are what a real training loop
    // would keep across batches.
    val weights: Tensor<f32, [4, 3]> = [
        [0.5, 0.0, 0.25],
        [0.0, 0.5, 0.25],
        [0.25, 0.0, 0.5],
        [0.0, 0.25, 0.5]
    ]
    val bias: Tensor<f32, [3]> = [1.0, 2.0, 3.0]
    val batch: Tensor<f32, [2, 4]> = [
        [1.0, 2.0, 3.0, 4.0],
        [4.0, 3.0, 2.0, 1.0]
    ]

    mut running: f32 = 0.0
    mut steps: i32 = 0

    pool training {
        println("bias total = {bias.sum():.2}")

        for step in 0..4 {
            pool batch_scratch {
                // Every tensor below is allocated in the arena: `@` contracts the
                // shared axis of a `[2, 4]` against a `[4, 3]`, the scalar stretches
                // across every element, and the `[3]` bias broadcasts down both rows.
                val activations = &batch @ &weights
                val scaled = &activations * 0.5
                val shifted = &scaled + &bias

                val mean = shifted.mean()
                val top = shifted.max()
                println("step {step}: mean={mean:.3} max={top:.3}")

                // Scalars may cross the arena boundary; the tensors above may not.
                running = running + mean
                steps = steps + 1
            }
        }
    }

    // Both arenas are back to their marks by here; the tally never left the stack.
    val tally = Tally { steps: steps, total: running }
    println(tally.render())

    steps
}
