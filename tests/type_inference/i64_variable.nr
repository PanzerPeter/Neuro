// Test: i64 type inference in variable declaration
// Literal 42 should infer as i64, not i32

func main() -> i32 {
    val x: i64 = 42
    return 42
}
