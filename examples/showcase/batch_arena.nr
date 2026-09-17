// Showcase: a batched forward pass run inside the pool allocator.
//
// This is an integration example: it exercises several features *together*
// rather than in isolation:
//   2D's `pool` block, taking its arena mark once per batch · 2D's `PoolAware`
//   opt-in and its reverse-order release sweep, which is what lets a type with a
//   destructor live in the arena ·
//   2B's tensor literal coercion and construction helpers · 2C's `@` matrix
//   multiplication and its by-value element-wise operators with scalar and row
//   broadcast · 2B's reductions, which read a batch back out as one number ·
//   a struct with a method on `&self` · a `for` loop over a range · string
//   interpolation with the format mini-language.
//
// The shape a training loop actually has: weights and running totals declared
// OUTSIDE the arena, so they survive it, and every per-batch intermediate built
// INSIDE it, so the batch's memory goes back in one store at the closing brace.
// The loop below allocates a product matrix, a scaled copy and a bias-broadcast
// sum per step, and its high-water mark never moves — which is the whole reason
// to reach for an arena over the heap in a loop like this.
//
// Four rules the block enforces are visible here. `running` and `steps` are an
// `f32` and an `i32`, so the inner block may write them: a scalar carries no
// address into the arena. No tensor declared before the `pool` is written from
// inside it, and no `break` or `return` leaves the block — the loop's exit is the
// loop's own, one level out. And `StepLog`, which the inner block builds and
// therefore owns, has a destructor: a type with one is refused by a pool unless it
// also implements `PoolAware`, because the arena's release is a single store and
// cannot run a destructor per object. Deleting the `impl PoolAware` block below is
// enough to make this program stop compiling. Inside the arena that trait REPLACES the
// destructor: the two notes each step builds are released by one reverse-order sweep at
// the block's brace, newest first, not one at a time as they go out of scope.
//
// The fourth is the string `headline` the inner block writes back out. A `string`
// carries a pointer, so the arena refuses one whose allocation it cannot account for:
// `headline = "step " + "{step}"` inside the block does not compile. What DOES compile
// is the line below it, because `summarize` is a declared function and its body is
// emitted outside every arena — the text it returns is ordinary heap memory the block's
// release never touches, which is why it is still readable after the closing brace. Only
// what the compiler can trace to the heap crosses; everything it cannot trace stays in.

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

// Called from inside the arena, written outside it. Every allocation this makes is
// emitted with its own body, so the text it hands back is heap memory and may be stored
// into a binding that outlives the pool.
func summarize(step: i32, mean: f32) -> string {
    return "kept step {step} at mean {mean:.3}"
}

// A per-step scratch note, built inside the arena. It holds a destructor, so it is
// exactly the shape a pool would otherwise refuse.
struct StepLog {
    step: i32
}

impl Drop for StepLog {
    func drop(&mut self) {
        println("  step {self.step} closed")
    }
}

// The opt-in. `register_with_pool` hands the arena the handle this value owns, and
// `bulk_release` is what the arena calls to give it back. Both run: the first at the
// point the value is constructed, the second during the arena's sweep at the closing
// brace — and never the `Drop` above, which is the whole trade the trait offers.
// The sweep walks registrations in reverse, so of the two notes each step builds, the
// one opened last is the one released first.
impl PoolAware for StepLog {
    func register_with_pool(&self, arena: &PoolHandle) {
    }
    func bulk_release(&mut self) {
        println("  note {self.step} released")
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
    mut headline: string = "no batches yet"

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
                val opened = StepLog { step: step }
                val queued = StepLog { step: step + 100 }
                println("step {opened.step}: mean={mean:.3} max={top:.3}")

                // Scalars may cross the arena boundary; the tensors above may not.
                running = running + mean
                steps = steps + 1

                // And so may this string, because the compiler can trace it to the heap.
                headline = summarize(step, mean)
            }
        }
    }

    // Both arenas are back to their marks by here; the tally never left the stack, and
    // `headline` still reads correctly because it never pointed into one.
    println(headline)
    val tally = Tally { steps: steps, total: running }
    println(tally.render())

    steps
}
