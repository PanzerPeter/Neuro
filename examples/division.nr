// NEURO Example: Division and modulo operations
// Demonstrates: division (/), modulo (%), arithmetic combinations

func divide(a: i32, b: i32) -> i32 {
    return a / b
}

func modulo(a: i32, b: i32) -> i32 {
    return a % b
}

func main() -> i32 {
    val dividend: i32 = 100
    val divisor: i32 = 7

    val quotient: i32 = divide(dividend, divisor)
    val remainder: i32 = modulo(dividend, divisor)

    val result: i32 = quotient + remainder
    return result
}
