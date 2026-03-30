// Test: Default to i32 without type annotation
// Literal 42 with no context should default to i32

func main() -> i32 {
    val x = 42
    return 42
}
