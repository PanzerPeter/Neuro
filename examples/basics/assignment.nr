// Neuro Example: assignment to mutable variables
// Demonstrates: `mut` bindings, reassignment, read-modify-write
//
// Each step prints the value the counter now holds; the final value is also
// returned as the exit code.

func test_assignment() -> i32 {
    mut counter: i32 = 0
    println("initial:        {counter}")

    counter = 10
    println("after assign:   {counter}")

    counter = counter + 5
    println("after add 5:    {counter}")

    return counter
}

func main() -> i32 {
    return test_assignment()
}
