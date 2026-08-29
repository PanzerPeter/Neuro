// Mutable methods — `&mut self` advances struct state in place.
//
// Demonstrates the ownership-gated receiver that landed in Phase 1.7:
//   `&mut self` methods write to `self.field` and the change is observed by a
//   later `&self` query. The accumulator is driven entirely through method
//   calls on a `mut` binding — no direct field assignment in `main`.
//
// The running total is printed after each `add`, so the in-place mutation is
// visible step by step.

struct Accumulator {
    total: i32
}

impl Accumulator {
    func new() -> Accumulator {
        Accumulator { total: 0 }
    }

    // Mutates in place: the write to `self.total` propagates to the caller.
    func add(&mut self, n: i32) {
        self.total = self.total + n
    }

    // Read-only query over the accumulated state.
    func get(&self) -> i32 {
        self.total
    }
}

func main() -> i32 {
    mut acc = Accumulator::new()

    acc.add(10)
    val after_first = acc.get()
    println("add(10) -> {after_first}")

    acc.add(15)
    val after_second = acc.get()
    println("add(15) -> {after_second}")

    acc.add(17)
    val after_third = acc.get()
    println("add(17) -> {after_third}")

    return acc.get()   // 10 + 15 + 17 == 42
}
