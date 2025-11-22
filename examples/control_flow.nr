// NEURO Example: Control flow
// Demonstrates: if/else, boolean logic, comparisons
//
// Note: Phase 1 has limitations with deeply nested control flow
// This example uses simple patterns that work within these limitations

func is_positive(n: i32) -> i32 {
    if n > 0 {
        return 1
    }
    return 0
}

func is_negative(n: i32) -> i32 {
    if n < 0 {
        return 1
    }
    return 0
}

func compare(x: i32, y: i32) -> i32 {
    if x > y {
        return 1
    }
    if x < y {
        return 2
    }
    return 3
}

func main() -> i32 {
    val num1: i32 = 42
    val num2: i32 = -10
    val num3: i32 = 15

    val pos_check: i32 = is_positive(num1)
    val neg_check: i32 = is_negative(num2)
    val cmp_result: i32 = compare(num1, num3)

    val result: i32 = pos_check + neg_check + cmp_result
    return result
}
