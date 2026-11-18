// Unit tests: a `@test` function takes nothing and returns nothing, and passes unless it
// panics. `neuroc run` enters at `main` as always; `neuroc test examples/basics/unit_tests.nr`
// builds the same file with a test entry instead and runs every `@test` function in its own
// process, so one failing `assert` stops only its own test. `std::testing::assert_close`
// compares floats within a tolerance, absolute near zero and relative above 1.

import std::testing::{assert_close}

// Hold `x` inside `[low, high]`.
func clamp(x: i32, low: i32, high: i32) -> i32 {
    if x < low { low } else if x > high { high } else { x }
}

func average(xs: [f64; 3]) -> f64 {
    (xs[0] + xs[1] + xs[2]) / 3.0
}

func main() -> i32 {
    println("clamp(15, 0, 10) = {clamp(15, 0, 10)}")
    println("average = {average([0.1, 0.2, 0.3]):.3}")
    clamp(42, 0, 10)
}

@test
func clamp_keeps_values_in_range() {
    assert(clamp(5, 0, 10) == 5)
    assert(clamp(-1, 0, 10) == 0)
    assert(clamp(11, 0, 10) == 10)
}

@test
func average_of_tenths() {
    // 0.1 + 0.2 + 0.3 is not exactly 0.6 in binary floating point, so `==` would fail here.
    assert_close(average([0.1, 0.2, 0.3]), 0.2, 1.0e-12)
}

@test
func main_returns_the_clamped_value() {
    assert(main() == 10)
}
