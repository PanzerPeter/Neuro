// Borrow discipline: what a live borrow forbids, and how code is written around it.
//
// Combines structs with `&mut self` / `&self` methods, arrays and `for` loops, and
// string interpolation with the borrowee rules of the memory model:
//
//   * while a `&mut` of a place is live, the place may not be read, written, or
//     moved through its own name — every access goes through the borrow;
//   * while any borrow of a place is live, the value may not be moved out of it,
//     because the borrow would be left pointing at storage the binding gave away.
//
// A borrow held by a binding lives until that binding leaves scope, so the usual
// shape is to confine the borrow to a block, or to read back through it with `*`.

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
    val doubled: i32 = *view + *view
    println("read through the borrow   = {doubled}")

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
