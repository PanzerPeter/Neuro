// NEURO Example: Factorial calculation using recursion
// Demonstrates: recursive function calls, if/else, arithmetic operations
//
// This program calculates factorial(5) = 120

func factorial(n: i32) -> i32 {
    val result: i32 = 0
    if n <= 1 {
        val result: i32 = 1
        return result
    } else {
        val prev: i32 = factorial(n - 1)
        val result: i32 = n * prev
        return result
    }
    return result
}

func main() -> i32 {
    val result: i32 = factorial(5)
    return result
}
