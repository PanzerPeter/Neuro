// A function call, a named binding for its result, and a formatted line on
// standard output. The smallest program that is more than `main`.

func add(a: i32, b: i32) -> i32 {
    return a + b
}

func main() -> i32 {
    val result = add(5, 3)
    println("add(5, 3) = {result}")
    return result
}
