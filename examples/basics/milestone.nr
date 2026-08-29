// Phase 1 Milestone Program
// Tests: functions, parameters, variable declarations, return values,
// function calls, and writing the result to standard output.

func add(a: i32, b: i32) -> i32 {
    return a + b
}

func main() -> i32 {
    val result = add(5, 3)
    println("add(5, 3) = {result}")
    return result
}
