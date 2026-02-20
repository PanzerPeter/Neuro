func sum_range(start: i32, end: i32) -> i32 {
    mut sum: i32 = 0

    for i in start..end {
        sum = sum + i
    }

    return sum
}

func main() -> i32 {
    return sum_range(0, 5)
}
