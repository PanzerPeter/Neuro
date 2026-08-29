// Neuro Example: Division and modulo operations
// Demonstrates: division (/), modulo (%), arithmetic combinations
//
// Prints both halves of the division and their sum, which is also the exit code.

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

    // Integer division truncates toward zero; the modulo carries what it dropped.
    println("{dividend} / {divisor} = {quotient}")
    println("{dividend} % {divisor} = {remainder}")

    val result: i32 = quotient + remainder
    println("sum       = {result}")
    return result
}
