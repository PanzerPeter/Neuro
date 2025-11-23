// Test assignment to mutable variables

func test_assignment() -> i32 {
    mut counter: i32 = 0
    counter = 10
    counter = counter + 5
    return counter
}

func main() -> i32 {
    return test_assignment()
}
