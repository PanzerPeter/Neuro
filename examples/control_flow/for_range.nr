// The half-open range `for` loop: `start..end` visits every value from `start` up to but
// not including `end`. Each visited value is printed, so the excluded bound shows in the
// output.

func sum_range(start: i32, end: i32) -> i32 {
    mut sum: i32 = 0

    for i in start..end {
        println("visit {i}")
        sum = sum + i
    }

    return sum
}

func main() -> i32 {
    val total = sum_range(0, 5)
    println("sum of 0..5 = {total}")
    return total
}
