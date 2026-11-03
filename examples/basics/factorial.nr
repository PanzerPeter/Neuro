// Neuro Example: Factorial calculation using recursion Demonstrates recursive calls,
// if/else, and arithmetic. The recursion prints each multiplication as it unwinds, so the
// output shows how factorial(5) is built.

func factorial(n: i32) -> i32 {
    val result: i32 = 0
    if n <= 1 {
        val result: i32 = 1
        println("factorial(1) = 1")
        return result
    } else {
        val prev: i32 = factorial(n - 1)
        val result: i32 = n * prev
        println("factorial({n}) = {n} * {prev} = {result}")
        return result
    }
    return result
}

func main() -> i32 {
    val result: i32 = factorial(5)
    return result
}
