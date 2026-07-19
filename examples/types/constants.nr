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

    // Function-body const folds at compile time
    const LOCAL_SCALE: i32 = 2
    val scaled: i32 = result * LOCAL_SCALE   // 23 * 2 = 46

    return apply_offset(scaled)              // 46 + 5 = 51
}
