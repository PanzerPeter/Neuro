@no_prelude

// Opting out of the implicit prelude.
//
// `@no_prelude` is written on the first line of a file — before any declaration, and
// never inside a `module { }` block, which is not a file. It takes away this file's
// prelude bindings: `Some`, `None`, `Ok`, and `Err` no longer name variants here, so a
// bare `Some(1)` is an error rather than an `Option`.
//
// On the *root* file it goes further and drops the prelude's declarations from the whole
// program — `Option` and `Result` are not declared anywhere — because the merged module
// namespace is flat, so those types are either in the program or absent from all of it.
// On a non-root module it takes the bindings only.
//
// What `@no_prelude` does *not* take away is `print` / `println`. Those are compiler
// builtins resolved by name, not prelude declarations, so they still work here — which
// is why this file can report its answer at all.

func triangular(n: i32) -> i32 {
    mut total: i32 = 0
    for step in 1..n + 1 {
        total += step
    }
    total
}

func main() -> i32 {
    val total = triangular(8)
    println("triangular(8) = {total}")
    println("no Option, no Result, still printing")
    total           // 1 + 2 + ... + 8 = 36
}
