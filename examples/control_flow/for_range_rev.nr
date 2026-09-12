// `.rev()` on a range: walk the same bounds from the top down.
//
// `.rev()` applies to a RANGE, not to a sequence, so it is written on the
// parenthesised range itself. It changes the order values arrive in and nothing
// else: the bounds, the element type, and which values the range names are the
// same as without it.
//
// The exclusive and inclusive spellings differ exactly where they always do.
// `(0..5).rev()` starts at 4, because 5 was never in the range; `(0..=5).rev()`
// starts at 5, because it was.
//
// Expected exit code: 0

func main() -> i32 {
    mut exclusive = ""
    for i in (0..5).rev() {
        exclusive = exclusive + "{i} "
    }
    println("(0..5).rev()      = {exclusive}")

    mut inclusive = ""
    for i in (0..=5).rev() {
        inclusive = inclusive + "{i} "
    }
    println("(0..=5).rev()     = {inclusive}")

    // An empty range is empty in either direction.
    mut ran = 0
    for i in (3..3).rev() {
        ran = ran + 1
    }
    println("(3..3).rev() runs = {ran}")

    // The position `.enumerate()` binds counts ITERATIONS, so it still climbs
    // from zero while the value it is paired with descends.
    mut pairs = ""
    for (position, value) in (10..14).rev().enumerate() {
        pairs = pairs + "{position}:{value} "
    }
    println("(10..14).rev()    = {pairs}")

    // Adapters sit above the reversal: the values arrive descending, and each
    // one is then filtered and mapped.
    mut doubled = ""
    for v in (0..6).rev().filter(|n: i32| -> bool { n % 2 == 1 }).map(|n: i32| -> i32 { n * 2 }) {
        doubled = doubled + "{v} "
    }
    println("odds doubled      = {doubled}")

    // The same adapter reads a tensor axis back to front. The result is a fresh
    // tensor, so the source keeps its own order.
    val samples: Tensor<i32, [5]> = [10, 20, 30, 40, 50]
    val newest_first: Tensor<i32, [5]> = samples[(0..5).rev()]
    println("samples[0]        = {samples[0]}")
    println("reversed[0]       = {newest_first[0]}")

    // A sub-range reverses within its own bounds, not the whole axis.
    val middle: Tensor<i32, [3]> = samples[(1..4).rev()]
    println("middle reversed   = {middle[0]} {middle[1]} {middle[2]}")

    return 0
}
