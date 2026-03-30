// Test: u32 type inference in function parameter
// Literal 100 should infer as u32

func foo(x: u32) -> u32 {
    x
}

func main() -> i32 {
    val result: u32 = foo(100)
    return 100
}
