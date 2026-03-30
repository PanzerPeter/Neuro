// NEURO Example: Simple arithmetic function
// Demonstrates Phase 1 syntax: functions, variables, and return statements

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
    val calculated: i32 = calculate(sum)
    return calculated
}
