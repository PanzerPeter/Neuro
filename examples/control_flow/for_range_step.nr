// `.step(n)` on a range: keep every `n`th value of the walk.
//
// Like `.rev()`, `.step(n)` applies to a RANGE, written on the parenthesised range
// itself, and `n` must be a positive integer of the range's own type. A constant
// stride below one is a compile error; a run-time one is checked when the loop
// starts, and a zero stride aborts rather than looping forever.
//
// `.rev()` goes underneath: `(0..10).rev().step(3)` starts at the LAST value and
// strides down (9, 6, 3, 0). The two orders are not the same walk, so only this
// one is accepted.
//
// Expected exit code: 0

func main() -> i32 {
    mut every_fifth = ""
    for i in (0..30).step(5) {
        every_fifth = every_fifth + "{i} "
    }
    println("(0..30).step(5)          = {every_fifth}")

    // An inclusive range still yields its bound when a stride lands on it.
    mut inclusive = ""
    for i in (0..=12).step(4) {
        inclusive = inclusive + "{i} "
    }
    println("(0..=12).step(4)         = {inclusive}")

    mut down = ""
    for i in (0..10).rev().step(3) {
        down = down + "{i} "
    }
    println("(0..10).rev().step(3)    = {down}")

    // The loop never adds past its bound, so a stride near the top of the type
    // still stops: 250 is the last value, not a wrap back to 4.
    mut last: u8 = 0
    for i in (0u8..255u8).step(10) {
        last = i
    }
    println("(0u8..255).step(10) ends = {last}")

    // `.enumerate()` counts iterations, not the stride.
    mut pairs = ""
    for (position, value) in (100..130).step(10).enumerate() {
        pairs = pairs + "{position}:{value} "
    }
    println("enumerated               = {pairs}")

    // On a tensor axis the stride divides the extent, rounding up, so seven
    // samples stepped by two leave four.
    val samples: Tensor<i32, [7]> = [10, 20, 30, 40, 50, 60, 70]
    val evens: Tensor<i32, [4]> = samples[(0..7).step(2)]
    val newest: Tensor<i32, [3]> = samples[(0..7).rev().step(3)]
    println("samples[(0..7).step(2)]  = {evens[0]} {evens[1]} {evens[2]} {evens[3]}")
    println("rev().step(3)            = {newest[0]} {newest[1]} {newest[2]}")

    return 0
}
