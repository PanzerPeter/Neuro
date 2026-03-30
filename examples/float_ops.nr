// NEURO Example: Floating-point operations
// Demonstrates: f64 type, float arithmetic
//
// Note: Float comparisons in if conditions are not yet supported in Phase 1
// This example shows float arithmetic without comparisons

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

    return 42
}
