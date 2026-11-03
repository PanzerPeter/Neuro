@no_prelude

// Opting out of the implicit prelude. `@no_prelude` goes on the first line of a file,
// before any declaration, and never inside a `module { }` block. It removes this file's
// prelude bindings: `Some`, `None`, `Ok` and `Err` no longer name variants here, so a bare
// `Some(1)` is an error. On the root file it also drops the prelude's declarations from the
// whole program, because the namespace is flat. On a non-root module it takes the bindings
// only.
//
// What `@no_prelude` does *not* take away is `print` / `println`. Those are compiler
// builtins resolved by name, so they still work here.

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
