// The `?` operator — propagate a failure out of the enclosing function.
//
// `expr?` unwraps an `Option<T>` / `Result<T, E>` to its payload, or leaves the
// function immediately carrying `None` / `Err(e)` on. The error travels as-is:
// there is no implicit conversion, so a callee's `E` must already be the caller's.
//
// Each call prints what it produced, so a short-circuit is visible as the
// fallback showing up in the output.
//
// Expected exit code: 129

func halve(n: i32) -> Result<i32, i32> {
    if n % 2 == 0 {
        Result::Ok(n / 2)
    } else {
        Result::Err(n)
    }
}

// Two propagations in a row: the first failure short-circuits the second.
func quarter(n: i32) -> Result<i32, i32> {
    val half = halve(n)?
    val rest = halve(half)?
    Result::Ok(rest)
}

func digit(n: i32) -> Option<i32> {
    if n >= 0 && n <= 9 {
        Option::Some(n)
    } else {
        Option::None
    }
}

// `None` propagates exactly like `Err`, rebuilt as this function's own `Option`.
func digit_sum(a: i32, b: i32) -> Option<i32> {
    val x = digit(a)?
    val y = digit(b)?
    Option::Some(x + y)
}

func main() -> i32 {
    val ok = quarter(40) ?? 0            // 40 -> 20 -> 10
    val failed = quarter(6) ?? 100       // halve(3) fails, so Err(3) leaves quarter()
    val summed = digit_sum(4, 5) ?? 0    // 9
    val missing = digit_sum(4, 55) ?? 7  // digit(55) is None, so the sum never happens

    println("quarter(40)       = {ok}")
    println("quarter(6)        = {failed}   (fallback: Err(3) propagated out)")
    println("digit_sum(4, 5)   = {summed}")
    println("digit_sum(4, 55)  = {missing}   (fallback: None propagated out)")

    // The forwarded payload is the original one, unchanged by the trip.
    val reason = match quarter(6) {
        Result::Ok(v) => v,
        Result::Err(e) => e              // 3 — the odd value halve() rejected
    }
    println("propagated Err    = {reason}")

    val total = ok + failed + summed + missing + reason
    println("total             = {total}")

    total
}
