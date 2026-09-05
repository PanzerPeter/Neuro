// Integer division throughput with a divisor the optimizer cannot pin down.
//
// `/` and `%` are the two operators that carry runtime guards — a zero divisor
// and `MIN / -1` are undefined for the hardware instruction, so the backend
// tests for them. Every divisor here comes out of a Vec, so no range analysis
// can fold those tests away, which makes this the worst case for their cost.
// The running remainder keeps each iteration dependent on the last, so the
// loop cannot be vectorized or folded.

func work(n: i32) -> i64 {
    mut divisors: Vec<i64> = Vec::new()
    mut d: i32 = 0
    while d < 64 {
        divisors.push((d as i64) + 3)
        d += 1
    }

    mut acc: i64 = 1
    mut i: i32 = 0
    while i < n {
        val k = divisors[i % 64]
        acc = ((acc + (i as i64)) / k) + ((acc * 7 + (i as i64)) % k) + 1
        i += 1
    }
    return acc
}

func main() -> i32 {
    val a = work(20000000)
    println("acc = {a}")
    return 0
}
