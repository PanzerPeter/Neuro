// Inline `module { }` blocks: a module that needs no file of its own.
//
// A block is a module in every sense a file is one — its items are private unless written
// with `export`, a qualified path reaches into it, and blocks nest. Use one for a grouping
// that belongs with the code around it; use a file when the unit is worth compiling apart.

module counter {
    export struct Tally {
        export hits: i32
    }

    impl Tally {
        func new() -> Tally {
            Tally { hits: 0 }
        }

        func record(&mut self, amount: i32) {
            self.hits = self.hits + step(amount)
        }
    }

    // Nested one level deeper: `counter::limits` from outside, `limits` from in here.
    module limits {
        export const CEILING: i32 = 30
    }

    export func capped(value: i32) -> i32 {
        if value > limits::CEILING {
            limits::CEILING
        } else {
            value
        }
    }

    // No `export`: the file below declares this block but is still outside it, so this
    // name is unreachable from `main` — that is the point of writing a block at all.
    func step(amount: i32) -> i32 {
        amount * 2
    }
}

// An import reaches into a block exactly as it reaches into a file.
import counter::{Tally}

func main() -> i32 {
    mut tally: Tally = Tally::new()
    tally.record(4)                          // 4 * 2 = 8
    tally.record(9)                          // 9 * 2 = 18, running total 26
    println("counter::Tally hits       = {tally.hits}")

    val within = counter::capped(tally.hits)          // 26 <= 30, so 26
    val over = counter::capped(41)                    // clamped to 30
    val ceiling = counter::limits::CEILING            // 30
    println("counter::capped(26)       = {within}")
    println("counter::capped(41)       = {over}")
    println("counter::limits::CEILING  = {ceiling}")

    val total = within + over - ceiling
    println("total                     = {total}")

    total                                    // 26 + 30 - 30 = 26
}
