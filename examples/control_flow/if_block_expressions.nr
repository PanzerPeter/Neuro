// `if` and bare blocks as *expressions* — each evaluates to a value rather than
// only steering control flow.
//
// Every branch result is printed, so which arm each call took can be read off
// the output.

func abs(n: i32) -> i32 {
    val result = if n >= 0 { n } else { 0 - n }
    result
}

func clamp(n: i32, lo: i32, hi: i32) -> i32 {
    val clamped = if n < lo { lo } else if n > hi { hi } else { n }
    clamped
}

func main() -> i32 {
    val a = abs(0 - 7)
    println("abs(-7)            = {a}")

    // A bare block is an expression too: its trailing expression is its value.
    val area = {
        val w: i32 = 6
        val h: i32 = 7
        w * h
    }
    println("block value        = {area}")

    val b = clamp(150, 0, 100)
    println("clamp(150, 0, 100) = {b}")

    val total = a + area + b
    println("total              = {total}")
    total
}
