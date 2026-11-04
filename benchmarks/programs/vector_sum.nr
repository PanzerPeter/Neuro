// Growable-collection throughput: fill a Vec, then sweep it repeatedly by reference.
// Measures push/grow cost, load cost, and whether the sweep vectorizes. The XOR
// against the outer counter stops the repeat loop from being folded into a single
// multiply.

func work(n: i32) -> i64 {
    mut v: Vec<i64> = Vec::new()
    for i in 0..n {
        v.push((i as i64) % 97)
    }
    mut acc: i64 = 0
    for r in 0i64..7000i64 {
        for x in &v {
            acc += *x ^ r
        }
    }
    return acc
}

func main() -> i32 {
    println("acc = {work(50000)}")
    return 0
}
