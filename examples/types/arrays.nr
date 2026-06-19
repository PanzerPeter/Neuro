// Fixed-size arrays `[T; N]` (§3.1): literals, indexing, element assignment,
// `.len()`, by-value iteration, and borrowed iteration `for x in &arr`.

func sum(xs: [i32; 5]) -> i32 {
    mut total: i32 = 0
    for x in xs {
        total = total + x
    }
    total
}

func main() -> i32 {
    mut data: [i32; 5] = [1, 2, 3, 4, 5]
    data[0] = 10                 // data is now [10, 2, 3, 4, 5]

    val total: i32 = sum(data)   // 10 + 2 + 3 + 4 + 5 = 24
    val n: u64 = data.len()      // 5

    mut borrowed: i32 = 0
    for x in &data {
        borrowed = borrowed + x  // 24
    }

    // data[2] = 3, n = 5; the two correction terms are 0 when total and the
    // borrowed sum are computed correctly. Exit code: 3 + 5 = 8.
    return data[2] + (n as i32) + (total - 24) + (borrowed - 24)
}
