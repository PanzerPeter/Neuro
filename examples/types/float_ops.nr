// Neuro Example: Floating-point operations
// Demonstrates: f64 type, float arithmetic, and Phase 1.5 compliant IEEE-754 tracking

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

    // IEEE-754 Ordered Float comparisons
    if a < b { return 1 }
    if b <= a {} else { return 2 }
    if b > a { return 3 }
    if a >= b {} else { return 4 }

    // NaN handling test
    val isnan: f64 = 0.0 / 0.0
    if isnan < b { return 5 }
    if isnan >= b { return 6 }
    if isnan == isnan { return 7 }

    return 42
}
