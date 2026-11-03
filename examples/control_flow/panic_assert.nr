// Panic-family builtins: `panic`, `assert` and `unreachable`. Every guard here holds, so
// the program runs to completion and prints what each guard saw. A failing guard writes
// "<message> at <file>:<line>:<col>" to stderr and aborts without unwinding, so nothing
// further reaches stdout.

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
    val half = checked_div(10, 2)
    println("checked_div(10, 2) = {half}")
    assert(half == 5)

    val n = non_negative(7)
    println("non_negative(7)    = {n}")
    assert(n == 7)

    // Never taken with these inputs; unreachable() documents an impossible state
    // and aborts if the invariant is ever violated.
    if n == 99 {
        unreachable()
    }

    val third = checked_div(21, 3)
    println("checked_div(21, 3) = {third}")
    println("every guard held")

    third                         // 7 -> process exit code
}
