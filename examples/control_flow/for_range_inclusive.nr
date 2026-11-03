// The inclusive range `for` loop: `start..=end` visits `end` as well, unlike `start..end`.
// Each value is printed, so the `5` at the end shows the upper bound is included.

func main() -> i32 {
    mut total: i32 = 0

    for i in 0..=5 {
        println("visit {i}")
        total = total + i
    }

    println("sum of 0..=5 = {total}")
    return total
}
