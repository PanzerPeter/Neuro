// Integer literal suffixes pin a literal's type without a variable annotation. Each
// suffixed literal is printed, including the hex and binary ones, so the suffix and the
// base show independently.

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

    println("255u8            = {small}")
    println("1000i16          = {medium}")
    println("1_000_000_000i64 = {wide}")
    println("42i64            = {x}")
    println("0xFFu8           = {y}")
    println("0b1010i32        = {z}")
    println("sum_bytes(10u8, 20u8) = {result}")

    return result - 30
}
