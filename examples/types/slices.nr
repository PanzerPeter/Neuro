// Borrowed slices `&[T]` / `&mut [T]`: one signature over a fixed-size array, a
// sub-range of it, and a `Vec<T>`; `.len()`, indexing, iteration, and writing
// through a mutable slice.

// A `&[i32]` is a (pointer, length) view. It does not care where the elements
// came from, so this one signature serves every source below.
func sum(xs: &[i32]) -> i32 {
    mut total: i32 = 0
    for x in xs {
        total = total + x
    }
    total
}

func largest(xs: &[i32]) -> i32 {
    if xs.len() == 0 { return 0 }
    mut best: i32 = xs[0]
    mut i: u64 = 1
    while i < xs.len() {
        if xs[i] > best { best = xs[i] }
        i = i + 1
    }
    best
}

// `&mut [T]` grants write access to the borrowed run. The owner sees the writes:
// the slice points into its buffer rather than copying it.
func double_each(xs: &mut [i32]) {
    mut i: u64 = 0
    while i < xs.len() {
        xs[i] = xs[i] * 2
        i = i + 1
    }
}

func main() -> i32 {
    val fixed: [i32; 4] = [1, 2, 3, 4]

    mut grown: Vec<i32> = Vec::new()
    grown.push(10)
    grown.push(20)

    val whole: i32 = sum(&fixed)               // 1 + 2 + 3 + 4 = 10
    val part: i32 = sum(fixed.slice(1..3))     // 2 + 3         =  5
    val vector: i32 = sum(&grown)              // 10 + 20       = 30
    println("sum(&fixed)            = {whole}")
    println("sum(fixed.slice(1..3)) = {part}")
    println("sum(&grown)            = {vector}")

    // `.slice(range)` copies nothing — it re-points the view into the same buffer.
    val tail = fixed.slice(2..4)
    println("tail.len()             = {tail.len()}")
    println("largest(tail)          = {largest(tail)}")

    for (i, v) in fixed.slice(1..4).enumerate() {
        println("  fixed[{i} + 1] = {v}")
    }

    mut scores: [i32; 3] = [1, 2, 3]
    double_each(&mut scores)
    println("after double_each      = {scores[0]}, {scores[1]}, {scores[2]}")

    // 10 + 5 + 30 = 45, minus the doubled first element (2) = 43.
    return whole + part + vector - scores[0]
}
