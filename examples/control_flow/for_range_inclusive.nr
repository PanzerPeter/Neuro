// The inclusive range `for` loop: `start..=end` visits `end` as well, which is
// the one difference from the half-open `start..end` form.
//
// The loop prints each value, so `5` showing up at the end is the whole point.

func main() -> i32 {
    mut total: i32 = 0

    for i in 0..=5 {
        println("visit {i}")
        total = total + i
    }

    println("sum of 0..=5 = {total}")
    return total
}
