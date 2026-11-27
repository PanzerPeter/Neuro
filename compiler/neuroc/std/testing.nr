// `std::testing`: what a `@test` function checks with beyond `assert`.

// Whether `actual` is within `tolerance` of `expected`. The difference is measured absolutely
// while `expected` is at most 1 in magnitude and relative to it above that, so one tolerance
// serves values near zero and large ones alike. Equal values are close, infinities included;
// NaN is close to nothing. Any float type serves, all three arguments the same one.
export func is_close<T: Float>(actual: T, expected: T, tolerance: T) -> bool {
    if actual == expected {
        return true
    }
    // `x - x` is 0 for a finite `x` and NaN for an infinity or a NaN, which only equality,
    // tested above, can match.
    if (actual - actual).is_nan() || (expected - expected).is_nan() {
        return false
    }
    val magnitude = expected.abs()
    val scale = if magnitude > 1.0 { magnitude } else { 1.0 }
    (actual - expected).abs() <= tolerance * scale
}

// Panic, naming both values, unless `actual` is within `tolerance` of `expected` as
// `is_close` measures it. The panic names the line that called `assert_close`.
@track_caller
export func assert_close<T: Float>(actual: T, expected: T, tolerance: T) {
    if !is_close(actual, expected, tolerance) {
        panic("assert_close: {actual} is not within {tolerance} of {expected}")
    }
}

@test
func is_close_measures_by_magnitude() {
    assert(is_close(0.1, 0.10005, 0.001))
    assert(is_close(1000.0, 1000.5, 0.001))
    assert(!is_close(0.001, 0.002, 0.0001))
}

@test
func is_close_treats_infinity_and_nan_by_identity() {
    val inf = 1.0 / 0.0
    val nan = inf - inf
    assert(is_close(inf, inf, 0.0))
    assert(!is_close(5.0, inf, 1.0))
    assert(!is_close(-inf, inf, 1.0))
    assert(!is_close(nan, nan, 1.0))
}
