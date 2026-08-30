// Showcase — an online mean accumulator.
//
// Integration example combining struct state with float math:
//   struct with mixed i32/f64 fields · direct field mutation on a `mut`
//   binding · &self query methods · f64 division · while loop · `as` cast ·
//   `.is_nan()` guarding the accumulator against a non-finite sample.
//
// This example advances the running state by assigning to the fields directly
// on the mutable binding in `main`, keeping the method side read-only (`&self`).
// For the in-place `&mut self` style, see `structs/mut_self_accumulator.nr`.

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

    // One NaN anywhere in the sum poisons every later mean, so the accumulator
    // is only trustworthy while its own mean is a number.
    func is_trustworthy(&self) -> bool {
        return !self.mean().is_nan()
    }
}

func main() -> i32 {
    mut stats = RunningStats::new()

    // Accumulate the samples 1.0, 2.0, ... 10.0 (sum 55, count 10).
    mut i: i32 = 1
    while i <= 10 {
        stats.sum = stats.sum + (i as f64)
        stats.count = stats.count + 1
        val running = stats.mean()
        println("sample {i:>2} -> mean {running:.3}")
        i += 1
    }

    // A poisoned reading. NaN propagates through every arithmetic operation it
    // touches, and no comparison can catch it (`bad == bad` is false), so the
    // sample is screened with `.is_nan()` before it can reach the accumulator.
    val zero: f64 = 0.0
    val bad: f64 = zero / zero
    if bad.is_nan() {
        println("rejected a non-finite sample; accumulator untouched")
    } else {
        stats.sum = stats.sum + bad
        stats.count = stats.count + 1
    }
    println("accumulator trustworthy = {stats.is_trustworthy()}")

    val avg = stats.mean()   // 55.0 / 10.0 == 5.5
    println("sum {stats.sum:.1} over {stats.count} samples -> mean {avg:.2}")

    // Truncated mean as the exit code.
    val code = avg as i32
    println("truncated exit code = {code}")
    return code              // 5
}
