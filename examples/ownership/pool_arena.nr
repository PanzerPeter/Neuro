// The `pool` block: scoped arena allocation. Allocations the block itself writes come from
// one contiguous bump arena, and leaving the block frees all of them in one step. A `pool`
// may carry a label (`pool batch { }`) that names the arena in diagnostics. Three rules are
// checked at compile time:
//   * a value that outlives the block may not be written from inside it unless it is provably off the arena. Scalars cross
//     freely. A `string`, collection or tensor the block allocated may not be stored into an earlier binding. A store whose
//     text is built in place (`summary = "round " + ...`) is routed to the heap instead.
//   * a `return` or `break` that leaves the block carries a number, `bool`, `char` or nothing.
//   * a value with a destructor needs a `PoolAware` impl. The arena sweeps those in reverse registration order at the closing
//     brace, replacing the destructor.
// Allocations made inside called functions stay on the heap.

// A round marker built inside the arena. It has a destructor, so a `pool` would refuse
// to own one, so the `impl PoolAware` beneath it is what makes this program compile.

struct Round {
    index: i32
}

impl Drop for Round {
    func drop(&mut self) {
        println("  round {self.index} marker closed")
    }
}

// The opt-in. A type that owns an external resource hands it to the arena in
// `register_with_pool`, which the arena calls where the value is constructed, and gives
// it back in `bulk_release`, which the arena calls during its sweep. The `Drop` above
// runs only outside a pool; inside one, the line below is what prints.
impl PoolAware for Round {
    func register_with_pool(&self, arena: &PoolHandle) {
    }
    func bulk_release(&mut self) {
        println("  round {self.index} marker swept")
    }
}

// Called from inside the pool below. Its own allocations are not the block's, so this
// function needs no annotation and behaves identically wherever it is called.
func describe(rows: i64, cols: i64) -> string {
    "{rows}x{cols}"
}

func main() -> i32 {
    mut total_rounds: i32 = 0
    mut summary: string = "no rounds yet"

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
                val marker = Round { index: round }
                val line = "  round {marker.index}: scratch tensor {shape}"
                println(line)

                // A scalar may be stored into the counter declared outside the pool.
                total_rounds = total_rounds + 1

                // And so may this, because `describe` allocates outside the arena.
                // Writing `summary = line` instead would not compile: `line` is the
                // block's own.
                summary = describe(round as i64, 16)

                // Written inside the block and kept outside it. The concatenation is
                // routed off the arena because `summary` outlives this brace, so the
                // text below survives both releases.
                summary = "round {round}, " + summary
            }
        }

        println("pool training closing, {total_rounds} rounds done")
    }

    // Everything both arenas held is released by now, in two stores, and `summary`
    // still reads correctly because it never pointed into either.
    println("last round shape {summary}")
    total_rounds
}
