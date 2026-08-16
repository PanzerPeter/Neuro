// Showcase — multi-file compilation working alongside prior features.
//
// Cumulative integration example combining:
//   file-based modules: a sibling module (`stats`), a directory module (`report`,
//   which holds a `mod.nr`), and its child (`report::format`)
//   · structs with `impl` methods and associated functions
//   · a generic function, monomorphized per type argument
//   · fixed-size arrays and `for`-in iteration
//   · a heap-backed `Vec<T>` that frees its buffer at scope exit
//   · `Option<T>` unwrapped with a lazy `??` fallback
//   · enums with pattern matching
//
//   examples/showcase/telemetry/
//     main.nr            <- the root module
//     stats.nr           <- module `stats`
//     report/
//       mod.nr           <- module `report`
//       format.nr        <- module `report::format`
//
// A qualified path is what pulls a module into the build; there is no import yet.

func main() -> i32 {
    val samples: [i32; 5] = [12, 40, 7, 33, 18]

    // A struct built by another module, its method called unqualified on the value.
    val summary: stats::Summary = stats::summarize(samples)
    val mean = summary.mean()                            // 110 / 5 = 22

    // The weight is an Option, and `??` supplies the fallback lazily.
    mut score: i32 = 0
    score += report::weight_of(mean) ?? 0                // Normal -> 2

    // An empty batch reports no band at all, so the fallback is what lands.
    val quiet: [i32; 5] = [0, 0, 0, 0, 0]
    val empty = stats::summarize(quiet)
    score += report::weight_of(empty.mean()) ?? 5        // -> 7

    // The generic helper from `stats`, at i32 and at bool.
    val louder = stats::pick(summary.peak > 30, summary.peak, mean)   // 40
    val flagged = stats::pick(mean > 20, true, false)                 // true

    // A grandchild module through its parent directory module.
    val weighted = report::format::scale(score, 4)                    // 28

    val bonus = if flagged { 2 } else { 0 }
    weighted + louder + summary.count + bonus            // 28 + 40 + 5 + 2 = 75
}
