// Panic-family builtins (§1.2): panic / assert / unreachable.
//
// All guards in this program hold, so it runs to completion and exits with code 7.
// Flip any condition (e.g. divide by zero, or `assert(false)`) and the matching
// builtin prints a diagnostic ("<message> at <file>:<line>:<col>") to stderr and
// aborts the process — the stack is NOT unwound.

// `panic` in a guard: returns i32 normally, but diverges on the error path.
func checked_div(a: i32, b: i32) -> i32 {
    if b == 0 {
        panic("division by zero")
    }
    a / b
}

// `panic` in tail (implicit-return) position is allowed because it diverges.
func non_negative(n: i32) -> i32 {
    if n >= 0 { n } else { panic("expected a non-negative value") }
}

func main() -> i32 {
    // assert: the condition holds, so execution continues silently.
    assert(checked_div(10, 2) == 5)

    val n = non_negative(7)
    assert(n == 7)

    // Never taken with these inputs; unreachable() documents an impossible state
    // and aborts if the invariant is ever violated.
    if n == 99 {
        unreachable()
    }

    checked_div(21, 3)            // 7 -> process exit code
}
