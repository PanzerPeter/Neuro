// Demonstrates integer literal type suffixes.
// Suffixes pin the type of a literal without a variable annotation.

func sum_bytes(a: u8, b: u8) -> i32 {
    return a as i32 + b as i32
}

func main() -> i32 {
    // Suffix overrides contextual inference
    val small:  u8  = 255u8
    val medium: i16 = 1000i16
    val wide:   i64 = 1_000_000_000i64

    // No annotation needed when suffix is present
    val x = 42i64
    val y = 0xFFu8
    val z = 0b1010i32

    // Suffix values usable in expressions and function calls
    val result = sum_bytes(10u8, 20u8)

    return result - 30
}
