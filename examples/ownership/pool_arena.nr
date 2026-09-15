// The `pool` block: scoped arena allocation.
//
// Inside a `pool`, the allocations the block itself writes come from one contiguous
// bump arena instead of the heap. Leaving the block moves the bump pointer back, which
// frees everything the body allocated in a single step: no per-object release walk, and
// no cost that grows with how many objects there were.
//
// A pool may carry a label (`pool batch { ... }`). Nothing refers to it — a `pool` is
// not a value and cannot be broken out of — so the label exists to name the arena in a
// diagnostic, which is what makes nested pools readable.
//
// Two rules keep the arena safe, both checked at compile time:
//
//   * Nothing that outlives the block may be written from inside it. Storing a
//     `string`, a collection or a tensor into a binding declared before the `pool`
//     would leave that binding pointing at bytes the block's exit reclaims. Scalars
//     are free to cross: an `i32` carries no address.
//   * `return`, `break` and `continue` may not leave the block. Each would jump past
//     the arena release, so the compiler asks for the exit to be written outside.
//
// The arena captures what the block writes, not what its callees write: a buffer a
// called function allocates belongs to whoever that function hands it to, so it stays
// an ordinary heap allocation. That costs the arena's speed on such a path and never
// its safety.

// Called from inside the pool below. Its own allocations are not the block's, so this
// function needs no annotation and behaves identically wherever it is called.
func describe(rows: i64, cols: i64) -> string {
    "{rows}x{cols}"
}

func main() -> i32 {
    mut total_rounds: i32 = 0

    pool training {
        // Allocated in the arena: the concatenation writes into the bump region.
        val run = "run-" + "042"
        println("pool training opened for {run}")

        for round in 0..3 {
            // A nested pool takes its own mark. Everything this block allocates is
            // gone at its closing brace, so the loop's memory use is flat however
            // many rounds it runs.
            pool round_scratch {
                val weights = Tensor::<f32, [16, 16]>::zeros()
                val shape = describe(16, 16)
                val line = "  round {round}: scratch tensor {shape}"
                println(line)

                // A scalar may be stored into the counter declared outside the pool.
                total_rounds = total_rounds + 1
            }
        }

        println("pool training closing, {total_rounds} rounds done")
    }

    // Everything both arenas held is released by now, in two stores.
    total_rounds
}
