// Neuro Example: Floating-point operations
// Demonstrates: f64 type, float arithmetic, IEEE-754 comparison ordering,
// `.is_nan()`: the only way to detect NaN, since every comparison against it is false,
// and float-to-integer conversion: `as` saturates, `.to_checked::<T>()` returns `None`

func calculate_f64(x: f64, y: f64) -> f64 {
    val sum: f64 = x + y
    val diff: f64 = x - y
    val product: f64 = x * y
    val quotient: f64 = x / y
    val result: f64 = sum + diff + product + quotient
    return result
}

func main() -> i32 {
    val a: f64 = 10.5
    val b: f64 = 2.5
    val result: f64 = calculate_f64(a, b)
    println("a                  = {a}")
    println("b                  = {b}")
    println("sum+diff+prod+quot = {result}")

    // IEEE-754 Ordered Float comparisons
    if a < b { return 1 }
    if b <= a {} else { return 2 }
    if b > a { return 3 }
    if a >= b {} else { return 4 }

    // NaN handling test: every comparison against NaN is false, including
    // NaN == NaN, which is what makes the ordering *partial*.
    val isnan: f64 = 0.0 / 0.0
    val lt = isnan < b
    val ge = isnan >= b
    val eq = isnan == isnan
    println("NaN < b            = {lt}")
    println("NaN >= b           = {ge}")
    println("NaN == NaN         = {eq}")

    if isnan < b { return 5 }
    if isnan >= b { return 6 }
    if isnan == isnan { return 7 }

    // Because every comparison against NaN is false, no operator can detect it.
    // `.is_nan()` is the documented test: true for NaN alone, and false for the
    // infinities, which are ordinary ordered values.
    val inf: f64 = 1.0 / 0.0
    println("NaN.is_nan()       = {isnan.is_nan()}")
    println("Inf.is_nan()       = {inf.is_nan()}")
    println("b.is_nan()         = {b.is_nan()}")

    if !isnan.is_nan() { return 8 }
    if inf.is_nan() { return 9 }
    if b.is_nan() { return 10 }

    // `as` truncates toward zero and clamps whatever the target cannot hold, NaN
    // included (it becomes 0). `.to_checked::<T>()` reports those cases as `None`.
    // A constant written straight into an out-of-range `as` is a compile warning.
    val big: f64 = 300.7
    val clamped = big as u8
    val checked = big.to_checked::<u8>() ?? 0u8
    val in_range = b.to_checked::<u8>() ?? 0u8
    val from_nan = isnan.to_checked::<i32>() ?? -1
    println("300.7 as u8        = {clamped}")
    println("300.7 to_checked   = {checked} (None)")
    println("2.5 to_checked     = {in_range}")
    println("NaN to_checked     = {from_nan} (None)")

    if clamped != 255u8 { return 11 }
    if checked != 0u8 { return 12 }
    if in_range != 2u8 { return 13 }
    if from_nan != -1 { return 14 }

    println("all comparisons behaved")
    return 42
}
