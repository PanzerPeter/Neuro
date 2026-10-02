// Integer division throughput with a divisor the optimizer cannot pin down.
//
// Rust guards `/` and `%` the same way Neuro does: a zero divisor and `MIN / -1`
// panic instead of reaching the hardware instruction. Every divisor here comes out
// of a Vec, so no range analysis can fold those tests away, which makes this the
// worst case for their cost. The running remainder keeps each iteration dependent
// on the last, so the loop cannot be vectorized or folded.

fn work(n: i32) -> i64 {
    let divisors: Vec<i64> = (0..64).map(|d| d + 3).collect();
    let mut acc: i64 = 1;
    for i in 0..n {
        let k = divisors[(i % 64) as usize];
        acc = ((acc + i as i64) / k) + ((acc * 7 + i as i64) % k) + 1;
    }
    acc
}

fn main() {
    println!("acc = {}", work(20000000));
}
