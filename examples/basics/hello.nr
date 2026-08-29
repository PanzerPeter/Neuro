// Neuro Example: Simple arithmetic function
// Demonstrates functions, variables, return statements, and printing each
// intermediate value on its way to the result.

func add(a: i32, b: i32) -> i32 {
    return a + b
}

func calculate(x: i32) -> i32 {
    val doubled: i32 = x * 2
    val result: i32 = doubled + 10
    return result
}

func main() -> i32 {
    val x: i32 = 5
    val y: i32 = 3

    val sum: i32 = add(x, y)
    println("add({x}, {y})    = {sum}")

    val calculated: i32 = calculate(sum)
    println("calculate({sum}) = {calculated}")

    return calculated
}
