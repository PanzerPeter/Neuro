// Compile-time constants
// Demonstrates module-level and function-body const declarations.

const MAX_RETRIES: i32 = 3
const BASE_VALUE: i32 = 10
const DOUBLED: i32 = BASE_VALUE * 2

func apply_offset(x: i32) -> i32 {
    const OFFSET: i32 = 5
    return x + OFFSET
}

func main() -> i32 {
    // Module-level consts visible everywhere
    val result: i32 = DOUBLED + MAX_RETRIES  // 20 + 3 = 23
    println("BASE_VALUE  = {BASE_VALUE}")
    println("DOUBLED     = {DOUBLED}   (BASE_VALUE * 2, folded)")
    println("MAX_RETRIES = {MAX_RETRIES}")
    println("sum         = {result}")

    // Function-body const folds at compile time
    const LOCAL_SCALE: i32 = 2
    val scaled: i32 = result * LOCAL_SCALE   // 23 * 2 = 46
    println("* LOCAL_SCALE = {scaled}")

    val offset = apply_offset(scaled)        // 46 + 5 = 51
    println("+ OFFSET      = {offset}")

    return offset
}
