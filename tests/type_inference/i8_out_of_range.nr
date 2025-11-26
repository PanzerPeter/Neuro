// Test: i8 out of range error
// Literal 300 does not fit in i8 (max 127)
// This should produce a compile error

func main() -> i32 {
    val x: i8 = 300
    return 0
}
