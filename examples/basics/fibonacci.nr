// Neuro Example: Fibonacci sequence calculation
// Demonstrates: recursive function calls, multiple recursion branches
//
// `main` prints the whole sequence it walks up to, then returns fibonacci(10)
// as the exit code.

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
    // Each term is computed from scratch — the naive recursion, printed term by term.
    for i in 0..=10 {
        val term: i32 = fibonacci(i)
        println("fibonacci({i}) = {term}")
    }

    val result: i32 = fibonacci(10)
    return result
}
