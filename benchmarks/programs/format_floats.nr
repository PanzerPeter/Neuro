// Float-to-text throughput: 600000 interpolated floating-point holes.
//
// Rendering a `f64` at a fixed precision is what a training loop does every time
// it reports a loss, and it is a different code path from `print_lines`'s integer
// hole — the digits come from the C library's conversion rather than from a digit
// loop. The value is irrational-looking on purpose: a short decimal expansion
// would let the conversion finish early and measure the wrong thing.

func main() -> i32 {
    mut i: i32 = 0
    while i < 600000 {
        val x = (i as f64) * 1.41421356237309 - 0.618033988749895
        println("loss {x:.6}")
        i += 1
    }
    return 0
}
