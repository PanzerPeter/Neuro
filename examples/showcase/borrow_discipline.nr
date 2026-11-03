// Showcase: what a live borrow forbids, and how code is written around it. Integration
// example: structs with `&mut self` and `&self` methods, arrays, `for` loops and
// interpolation, and reading through a borrow with no `*`. While a `&mut` of a place is
// live, the place may not be read, written or moved through its own name. While any borrow
// is live, the value may not be moved out. A borrow lasts until its binding leaves scope,
// so confine it to a block or read back through it.

struct Accumulator {
    total: i32,
    count: i32
}

impl Accumulator {
    func new() -> Accumulator {
        Accumulator { total: 0, count: 0 }
    }

    func add(&mut self, sample: i32) {
        self.total = self.total + sample
        self.count = self.count + 1
    }

    func mean(&self) -> i32 {
        if self.count == 0 {
            return 0
        }
        return self.total / self.count
    }
}

// Scale the referent in place; the write is visible at the caller's binding.
func scale(n: &mut i32, factor: i32) {
    *n = *n * factor
}

// Comparisons read through the borrows, so this is written as it would be over owned
// values.
func in_band(x: &i32, lo: &i32, hi: &i32) -> bool {
    x >= lo && x <= hi
}

// A method call reads its receiver through both borrows.
func doubled_mean(acc: & &Accumulator) -> i32 {
    acc.mean() * 2
}

func main() -> i32 {
    mut acc: Accumulator = Accumulator::new()
    val samples: [i32; 5] = [3, 9, 12, 6, 30]
    for sample in samples {
        acc.add(sample)
    }
    val mean: i32 = acc.mean()
    println("mean of {samples.len()} samples         = {mean}")

    // A `&mut` confined to a block ends at the closing brace, so `running` is
    // frozen against access through its own name only while `r` is alive.
    mut running: i32 = mean
    if true {
        val r: &mut i32 = &mut running
        scale(r, 2)
    }
    println("after the scoped &mut     = {running}")

    // While a `&mut` IS live, the value is read back through the borrow.
    val view: &mut i32 = &mut running
    val doubled: i32 = view + view
    println("read through the borrow   = {doubled}")

    val lo: i32 = 5
    val hi: i32 = 12
    mut in_range: i32 = 0
    for sample in samples {
        if in_band(&sample, &lo, &hi) {
            in_range += 1
        }
    }
    println("samples in [{lo}, {hi}]        = {in_range}")
    val acc_view: &Accumulator = &acc
    println("mean doubled via & &      = {doubled_mean(&acc_view)}")

    // A `string` cannot be moved out from under a live borrow, so the borrow is
    // taken, used, and finished inside its own block before the move.
    val label: string = "samples"
    if true {
        val borrowed: &string = &label
        println("label length              = {borrowed.len()}")
    }
    val owned: string = label
    println("label moved, now owned    = {owned}")

    return doubled
}
