// Showcase — multi-file compilation and `import` working alongside prior features.
//
// Cumulative integration example combining:
//   file-based modules: a sibling module (`stats`), a directory module (`report`,
//   which holds a `mod.nr`), and its child (`report::format`)
//   · `import` in all its shapes: a name list, a rename, a module alias, and variants
//   · structs with `impl` methods and associated functions
//   · a generic function, monomorphized per type argument
//   · fixed-size arrays and `for`-in iteration
//   · a heap-backed `Vec<T>` that frees its buffer at scope exit
//   · `Option<T>` unwrapped with a lazy `??` fallback and by pattern
//   · enums with pattern matching
//   · `export` visibility: each module publishes a surface and keeps the rest private —
//     `Summary.total` and `report`'s `Band` never leave the file that declares them
//   · an inline `module { }` block: logical grouping inside one file, under the same
//     visibility rule a file module follows
//   · `export import`: `report` re-exports its child's `clamp`, so a caller reaches it as
//     `report::clamp` without ever naming `report::format`
//
//   examples/showcase/telemetry/
//     main.nr            <- the root module, plus the inline module `scoring`
//     stats.nr           <- module `stats`
//     report/
//       mod.nr           <- module `report`
//       format.nr        <- module `report::format`

// Names taken from a sibling module, one of them renamed.
import ./stats::{Summary, summarize, pick as choose}

// A name from the directory module, plus its child module under a shorter name.
import report::{weight_of}
import report::format as fmt

// Prelude enum variants, so the match arms below need no `Option::` prefix.
import Option::{Some, None}

// An inline module: a grouping worth a name but not a file. Its items follow the same
// rule a file's do — private unless written with `export`.
module scoring {
    export const BONUS: i32 = 2

    export func combine(weighted: i32, louder: i32, count: i32, flagged: bool) -> i32 {
        weighted + louder + count + adjust(flagged)
    }

    // No `export`: how the bonus is decided stays inside the block, so `main` below
    // cannot name it even though it is written in the same file.
    func adjust(flagged: bool) -> i32 {
        if flagged {
            BONUS
        } else {
            0
        }
    }
}

func main() -> i32 {
    val samples: [i32; 5] = [12, 40, 7, 33, 18]

    // A struct built by another module, its method called unqualified on the value.
    val summary: Summary = summarize(samples)
    val mean = summary.mean()                            // 110 / 5 = 22

    // The weight is an Option, and `??` supplies the fallback lazily.
    mut score: i32 = 0
    score += weight_of(mean) ?? 0                        // Normal -> 2

    // An empty batch reports no band at all, so the fallback arm is what lands.
    val quiet: [i32; 5] = [0, 0, 0, 0, 0]
    val empty = summarize(quiet)
    score += match weight_of(empty.mean()) {
        Some(weight) => weight,
        None         => 5
    }                                                    // -> 7

    // The renamed generic helper from `stats`, at i32 and at bool.
    val louder = choose(summary.peak > 30, summary.peak, mean)   // 40
    val flagged = choose(mean > 20, true, false)                 // true

    // A grandchild module, reached through the alias its import bound.
    val weighted = fmt::scale(score, 4)                          // 28

    // `clamp` lives in that same grandchild, but `report` re-exports it — so this route
    // stops one level short and never mentions `format`.
    val capped = report::clamp(weighted, 30)                     // 28

    // The inline module does the final tally; its private `adjust` supplies the bonus.
    scoring::combine(capped, louder, summary.count, flagged)     // 28 + 40 + 5 + 2 = 75
}
