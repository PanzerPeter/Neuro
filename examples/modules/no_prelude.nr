@no_prelude

// Opting out of the implicit prelude. `@no_prelude` goes on the first line of a file,
// before any declaration, and never inside a `module { }` block. It takes this file's
// prelude names away: `Option`, `Result`, `Some`, `None`, `Ok` and `Err` are no longer in
// scope here, so a bare `Some(1)` is an error. Other files of the program keep them, and
// the standard library is compiled with the program either way.
//
// The prelude is the module `std::prelude`, so a file that opted out imports exactly what
// it wants from it, variants included, or writes a full path.
import std::prelude::{Option, Some, None}

// What `@no_prelude` does *not* take away: `print` / `println`, which are compiler
// builtins, and the traits the language dispatches through (`Iterator`, `Clone`, ...).

func triangular(n: i32) -> i32 {
    mut total: i32 = 0
    for step in 1..n + 1 {
        total += step
    }
    total
}

// `Option` came in through the import above.
func first_square_over(limit: i32) -> Option<i32> {
    for n in 1..limit {
        if n * n > limit {
            return Some(n)
        }
    }
    None
}

// `Result` was not imported, so it is written by its path.
func checked(n: i32) -> std::prelude::Result<i32, i32> {
    if n < 0 {
        std::prelude::Result::Err(n)
    } else {
        std::prelude::Result::Ok(n)
    }
}

func main() -> i32 {
    val total = triangular(8)
    val root = first_square_over(30) ?? 0
    val kept = checked(total) ?? 0
    println("triangular(8)         = {total}")
    println("first_square_over(30) = {root}")
    println("checked(36)           = {kept}")
    kept + root           // 36 + 6 = 42
}
