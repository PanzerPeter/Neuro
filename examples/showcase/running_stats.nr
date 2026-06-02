// Showcase — an online mean accumulator.
//
// Integration example combining struct state with float math:
//   struct with mixed i32/f64 fields · direct field mutation on a `mut`
//   binding · &self query method · f64 division · while loop · `as` cast.
//
// Methods take `&self` only (`&mut self` is a later phase), so the running
// state is advanced by assigning to the fields directly on the mutable
// binding in `main`, and the method side stays read-only.

struct RunningStats {
    count: i32,
    sum: f64
}

impl RunningStats {
    func new() -> RunningStats {
        RunningStats { count: 0, sum: 0.0 }
    }

    // Arithmetic mean; guards against the empty case.
    func mean(&self) -> f64 {
        if self.count == 0 { return 0.0 }
        return self.sum / (self.count as f64)
    }
}

func main() -> i32 {
    mut stats = RunningStats::new()

    // Accumulate the samples 1.0, 2.0, ... 10.0 (sum 55, count 10).
    mut i: i32 = 1
    while i <= 10 {
        stats.sum = stats.sum + (i as f64)
        stats.count = stats.count + 1
        i += 1
    }

    val avg = stats.mean()   // 55.0 / 10.0 == 5.5

    // Truncated mean as the exit code.
    return avg as i32        // 5
}
