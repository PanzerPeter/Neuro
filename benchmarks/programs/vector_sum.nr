// Growable-collection throughput: fill a Vec, then sweep it repeatedly with an
// index. Measures push/grow cost, indexed load cost, and whether the sweep
// vectorizes. The XOR against the outer counter stops the repeat loop from
// being folded into a single multiply.

func work(n: i32) -> i64 {
    mut v: Vec<i64> = Vec::new()
    mut i: i32 = 0
    while i < n {
        v.push((i as i64) % 97)
        i += 1
    }
    mut acc: i64 = 0
    mut r: i32 = 0
    while r < 7000 {
        mut j: i32 = 0
        while j < n {
            acc += v[j] ^ (r as i64)
            j += 1
        }
        r += 1
    }
    return acc
}
func main() -> i32 {
    val a = work(50000)
    println("acc = {a}")
    return 0
}
