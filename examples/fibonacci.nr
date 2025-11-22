// NEURO Example: Fibonacci sequence calculation
// Demonstrates: recursive function calls, multiple recursion branches
//
// This program calculates fibonacci(10) = 55

func fibonacci(n: i32) -> i32 {
    val result: i32 = 0
    if n <= 1 {
        return n
    } else {
        val prev1: i32 = fibonacci(n - 1)
        val prev2: i32 = fibonacci(n - 2)
        val result: i32 = prev1 + prev2
        return result
    }
    return result
}

func main() -> i32 {
    val result: i32 = fibonacci(10)
    return result
}
