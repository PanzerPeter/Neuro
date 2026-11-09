// Showcase: multi-file compilation and `import` alongside earlier features. Integration
// example: a sibling module, a directory module with a child, `import` in its three shapes
// (a name list, a rename, and a module alias), the prelude, `export` visibility, an inline
// `module { }` block, `export import` re-exports, and module namespaces: a name declared in
// two modules, a path in a pattern and an `impl` header, and a local shadowing an import.
//
//   showcase/telemetry/
//     main.nr          <- this file, the root module, plus the inline module `scoring`
//     stats.nr         <- module `stats`
//     report/
//       mod.nr         <- module `report`
//       format.nr      <- module `report::format`

// Names taken from a sibling module, one of them renamed.

import ./stats::{Summary, summarize, pick as choose}

// A name from the directory module, plus its child module under a shorter name.
import report::{weight_of}
import report::format as fmt

// No import brings `Some` and `None` into scope: the implicit prelude binds them in every
// module, so the match arms below name them directly. `stats` shows the other side of the
// rule: a file that wants none of it writes `@no_prelude` on its first line.

// An inline module: a grouping worth a name but not a file. Its items follow the same
// rule a file's do: private unless written with `export`.
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

// `report::format` declares a `scale` too, reached below as `fmt::scale`. Every module is
// a namespace, so the two are different functions and neither has to be renamed.
func scale(value: i32) -> i32 {
    value + 1
}

// A trait from another module, named by its path in the impl header. `doubled` is not
// written here: it is the trait's default, declared in `report`.
struct Reading {
    level: i32
}

impl report::Weighted for Reading {
    func weight(&self) -> i32 {
        self.level
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

    // `clamp` lives in that same grandchild, but `report` re-exports it, so this route
    // stops one level short and never mentions `format`.
    val capped = report::clamp(weighted, 30)                     // 28

    println("stats::summarize -> mean       = {mean}")
    println("weight_of via prelude Option   = {score}")
    println("choose<i32> renamed from stats = {louder}")
    println("choose<bool> same template     = {flagged}")
    println("fmt::scale (module alias)      = {weighted}")
    println("report::clamp (re-export)      = {capped}")

    // The inline module does the tally; its private `adjust` supplies the bonus.
    val tally = scoring::combine(capped, louder, summary.count, flagged)
    println("scoring::combine (inline mod)  = {tally}")         // 28 + 40 + 5 + 2 = 75

    // An enum from another module, matched by its path.
    val level = match report::band_for(mean) {
        report::Band::Quiet  => 0,
        report::Band::Normal => 1,
        report::Band::Loud   => 3
    }                                                           // mean 22 is Normal: 1
    val reading = Reading { level: level }
    val bumped = scale(reading.doubled())                       // root's scale(2) = 3

    // `choose` was imported from `stats`; from here on it is this local instead.
    val choose = bumped * 2                                     // 6
    println("report::Band::Normal level     = {level}")
    println("scale(doubled) (root's scale)  = {bumped}")
    println("choose (local over the import) = {choose}")

    tally + choose                                              // 75 + 6 = 81
}
