// NEURO Extended Primitive Types Example
// Demonstrates i8, i16, u8, u16, u32, u64 types

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
    return 0
}
