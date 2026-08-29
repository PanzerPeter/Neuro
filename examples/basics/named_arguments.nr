// Neuro Example: Named arguments
// Demonstrates: passing arguments by name, external labels, positional-only `_`
//
// All three spellings of the same call print the same number, which is the
// point of the feature: the name is a call-site convenience, not a new function.

// An ordinary parameter may be passed positionally or by its own name.
func blend(base: i32, accent: i32, weight: i32) -> i32 {
    return base * 100 + accent * 10 + weight
}

// `external internal:` requires the name at the call site. The body uses the
// internal name; the caller writes the external one.
// `_ value:` is the opposite: the name is never written by the caller.
func clamp(_ value: i32, min lo: i32, max hi: i32) -> i32 {
    if value < lo {
        return lo
    }
    if value > hi {
        return hi
    }
    return value
}

func main() -> i32 {
    // Named arguments may appear in any order, and any number of positional
    // arguments may come first.
    val positional: i32 = blend(1, 2, 3)
    val named: i32 = blend(weight: 3, base: 1, accent: 2)
    val mixed: i32 = blend(1, weight: 3, accent: 2)

    println("blend(1, 2, 3)                       = {positional}")
    println("blend(weight: 3, base: 1, accent: 2) = {named}")
    println("blend(1, weight: 3, accent: 2)       = {mixed}")

    if positional != named {
        return 1
    }
    if positional != mixed {
        return 2
    }

    // `min` and `max` must be named; `99` must not be.
    val bounded: i32 = clamp(99, max: 40, min: 5)
    println("clamp(99, max: 40, min: 5)           = {bounded}")

    return bounded
}
