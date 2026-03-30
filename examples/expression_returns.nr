// Example: Expression-Based Returns in NEURO
// Demonstrates the new Phase 1 feature: implicit returns

// Simple expression-based return
func double(x: i32) -> i32 {
    x * 2
}

// Expression-based return with statements before
func calculate(a: i32, b: i32) -> i32 {
    val sum: i32 = a + b
    val product: i32 = a * b
    sum + product
}

// Comparison expression as return
func is_positive(x: i32) -> bool {
    x > 0
}

// Logical expression as return
func both_positive(a: i32, b: i32) -> bool {
    a > 0 && b > 0
}

// Function call as expression return
func quad(x: i32) -> i32 {
    double(double(x))
}

// Main function demonstrating the feature
func main() -> i32 {
    val result1: i32 = double(5)
    val result2: i32 = calculate(3, 4)
    val result3: i32 = quad(2)

    result1 + result2 + result3
}
