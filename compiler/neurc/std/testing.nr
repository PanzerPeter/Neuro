// `std::testing`: what a `@test` function checks with beyond `assert`.

// Whether `actual` is within `tolerance` of `expected`. The difference is measured absolutely
// while `expected` is at most 1 in magnitude and relative to it above that, so one tolerance
// serves values near zero and large ones alike. Equal values are close, infinities included;
// NaN is close to nothing.
export func is_close(actual: f64, expected: f64, tolerance: f64) -> bool {
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
// `is_close` measures it.
export func assert_close(actual: f64, expected: f64, tolerance: f64) {
    if !is_close(actual, expected, tolerance) {
        panic("assert_close: {actual} is not within {tolerance} of {expected}")
    }
}
