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
//
//   examples/showcase/telemetry/
//     main.nr            <- the root module
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

    val bonus = if flagged { 2 } else { 0 }
    weighted + louder + summary.count + bonus            // 28 + 40 + 5 + 2 = 75
}
