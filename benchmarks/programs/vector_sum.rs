// Growable-collection throughput: fill a Vec, then sweep it repeatedly. Measures
// push/grow cost, load cost, and whether the sweep vectorizes. The XOR against the
// outer counter stops the repeat loop from being folded into a single multiply.

fn work(n: i32) -> i64 {
    let mut v: Vec<i64> = Vec::new();
    for i in 0..n {
        v.push(i as i64 % 97);
    }
    let mut acc: i64 = 0;
    for r in 0..7000i64 {
        acc += v.iter().map(|&x| x ^ r).sum::<i64>();
    }
    acc
}

fn main() {
    println!("acc = {}", work(50000));
}
