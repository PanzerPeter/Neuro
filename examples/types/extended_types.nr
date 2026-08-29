// Neuro Extended Primitive Types Example
// Demonstrates i8, i16, u8, u16, u32, u64 types
//
// Every helper is called and its answer printed, so each width is exercised
// rather than merely declared. `main` returns 0: the assertions are the printed
// values, not the exit code.

func test_i8_arithmetic(a: i8, b: i8) -> i8 {
    val sum: i8 = a + b
    val diff: i8 = a - b
    val prod: i8 = a * b
    return sum
}

func test_i16_operations(a: i16, b: i16) -> i16 {
    val result: i16 = a / b
    return result
}

func test_u8_arithmetic(a: u8, b: u8) -> u8 {
    val sum: u8 = a + b
    return sum
}

func test_u16_operations(a: u16, b: u16) -> u16 {
    val diff: u16 = a - b
    return diff
}

func test_u32_division(a: u32, b: u32) -> u32 {
    val quot: u32 = a / b
    val rem: u32 = a % b
    return quot
}

func test_u64_comparison(a: u64, b: u64) -> bool {
    if a > b {
        return true
    }
    return false
}

func test_signed_comparison(a: i8, b: i8) -> bool {
    return a < b
}

func test_unsigned_comparison(a: u32, b: u32) -> bool {
    return a >= b
}

func main() -> i32 {
    // The i8 helper also computes `a * b` internally, so the inputs are chosen to
    // keep that product inside i8 — a debug build traps on overflow.
    val i8_sum: i8 = test_i8_arithmetic(10i8, 3i8)            // 13
    val i16_quot: i16 = test_i16_operations(1000i16, 8i16)    // 125
    val u8_sum: u8 = test_u8_arithmetic(200u8, 55u8)          // 255
    val u16_diff: u16 = test_u16_operations(60000u16, 500u16) // 59500
    val u32_quot: u32 = test_u32_division(1000000u32, 7u32)   // 142857

    println("i8   10 + 3           = {i8_sum}")
    println("i16  1000 / 8         = {i16_quot}")
    println("u8   200 + 55         = {u8_sum}")
    println("u16  60000 - 500      = {u16_diff}")
    println("u32  1000000 / 7      = {u32_quot}")

    val u64_gt: bool = test_u64_comparison(9000000000u64, 42u64)
    val i8_lt: bool = test_signed_comparison(-5i8, 3i8)
    val u32_ge: bool = test_unsigned_comparison(7u32, 7u32)

    println("u64  9000000000 > 42  = {u64_gt}")
    println("i8   -5 < 3           = {i8_lt}")
    println("u32  7 >= 7           = {u32_ge}")

    return 0
}
