// The `??` operator: unwrap a fallible value or fall back. `??` yields the inner `T` of an
// `Option<T>` or `Result<T, E>`, or the right-hand side when the value is absent or the
// call failed. For a `Result` the error is discarded; `match` on the value when the reason
// matters. Every result is printed, so the output shows which side of each `??` ran.

func lookup(key: i32) -> Option<i32> {
    if key == 1 {
        Option::Some(20)
    } else {
        Option::None
    }
}

func divide(a: i32, b: i32) -> Result<i32, i32> {
    if b == 0 {
        Result::Err(404)
    } else {
        Result::Ok(a / b)
    }
}

// A fallback may be any expression producing the payload type: including a call.
func default_reading() -> i32 {
    3
}

func main() -> i32 {
    // Option: present unwraps, absent falls back.
    val present = lookup(1) ?? 0        // 20
    val absent = lookup(7) ?? 5         // 5
    println("lookup(1) ?? 0                = {present}")
    println("lookup(7) ?? 5                = {absent}")

    // Result: the Ok payload unwraps, and the Err payload (404) never surfaces.
    val ok = divide(24, 4) ?? 0         // 6
    val failed = divide(1, 0) ?? 4      // 4
    println("divide(24, 4) ?? 0            = {ok}")
    println("divide(1, 0) ?? 4             = {failed}")

    // `??` associates right-to-left, so `a ?? b ?? c` is `a ?? (b ?? c)`. Each
    // fallback is only evaluated once every operand before it has come up absent.
    val chained = lookup(7) ?? lookup(1) ?? 99      // 20
    println("lookup(7) ?? lookup(1) ?? 99  = {chained}")

    // The fallback is lazy: `default_reading()` is not called here, because the
    // left-hand value is present.
    val lazy = lookup(1) ?? default_reading()       // 20
    println("lookup(1) ?? default_reading()= {lazy}")

    val total = present + absent + ok + failed + chained + lazy
    println("total                         = {total}")

    total // 75
}
