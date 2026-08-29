// Tuples and destructuring.
//
// A tuple `(T1, T2, ...)` is an anonymous, fixed-size, heterogeneous aggregate.
// Elements are read by constant index (`t.0`, `t.1`) and bound by destructuring.
// Tuple elements are restricted to Copy types for now, so a tuple is itself Copy.

// Tuples cross function boundaries — here a tuple is the return type.
func min_max(a: i32, b: i32) -> (i32, i32) {
    if a < b { (a, b) } else { (b, a) }
}

func main() -> i32 {
    // Tuple type annotation + literal + index access.
    val pair: (i32, i32) = (12, 30)
    val sum = pair.0 + pair.1            // 42

    // Destructuring bind with a `_` wildcard.
    val (_, hi) = min_max(9, 4)          // hi = 9

    // Nested literal and nested destructuring.
    val nested = ((1, 2), 3)
    val ((a, b), c) = nested             // a=1 b=2 c=3

    // Heterogeneous elements and chained index on a temporary projection.
    val mixed: (i32, bool) = (5, true)
    val bonus = if mixed.1 { mixed.0 } else { 0 }   // 5

    println("pair.0 + pair.1        = {sum}")
    println("val (_, hi) = min_max   -> {hi}")
    println("val ((a, b), c)         -> {a}, {b}, {c}")
    println("mixed.0 / mixed.1       -> {mixed.0}, {mixed.1}")
    println("bonus                   = {bonus}")

    // 42 + 9 + (1+2+3) + 5 = 62
    val total = sum + hi + a + b + c + bonus
    println("total                   = {total}")
    return total
}
