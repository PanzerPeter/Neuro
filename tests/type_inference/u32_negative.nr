// Test: u32 too large error
// Literal 5000000000 is too large for u32 (max 4294967295)
// This should produce a compile error

func main() -> i32 {
    val x: u32 = 5000000000
    return 0
}
