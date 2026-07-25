// Showcase — deterministic `Drop` and labeled loop exit working together with
// earlier features.
//
// Cumulative integration example combining:
//   `impl Drop` destructors running at scope exit (reverse declaration order)
//   · a labeled `break` leaving a nested loop from the inside
//   · mutable borrows `&mut i32` shared by the guards
//   · structs with `&self` methods · @derive(Copy)
//   · fixed-size arrays with indexed reads · `match` with range and `_` patterns
//   · if-expressions and implicit returns
//
// `Drop` and labeled breaks are the two features this file exists to exercise in
// combination: the labeled `break` unwinds out of *two* loops at once, and the
// scope guard must still run its destructor exactly once on that path. Because a
// labeled break is a normal scope exit (not a panic), the destructors do run.

struct ScanLog {
    closed: &mut i32
}

impl Drop for ScanLog {
    func drop(&mut self) {
        *self.closed = *self.closed + 1
    }
}

@derive(Copy, Clone)
struct Sensor {
    base: i32,
    gain: i32
}

impl Sensor {
    func reading(&self, tick: i32) -> i32 {
        self.base + self.gain * tick
    }
}

// 0 = nominal, 1 = elevated, 2 = critical.
func classify(value: i32) -> i32 {
    match value {
        0..=9 => 0,
        10..=19 => 1,
        _ => 2
    }
}

func main() -> i32 {
    mut closed: i32 = 0
    mut score: i32 = 0
    mut ticks: i32 = 0

    {
        // Two guards over the same counter; both must fire at the end of this
        // block, including on the labeled-break path below.
        val outer_log = ScanLog { closed: &mut closed }
        val inner_log = ScanLog { closed: &mut closed }

        val sensor = Sensor { base: 2, gain: 8 }
        val limits = [4, 12, 30]

        sweep: for tick in 0..4 {
            for slot in 0..3 {
                ticks = ticks + 1

                val value = sensor.reading(tick)
                val grade = classify(value)

                if value > limits[slot] {
                    score = score + grade
                }

                // Critical reading ends the whole sweep, not just this inner loop.
                if grade == 2 {
                    break sweep
                }
            }
        }
    }

    // Readings are 2, 10, 18, 26 — the last is critical, so the labeled `break`
    // fires on tick 3 after 10 inner iterations, with score 5. Both guards still
    // run their destructors on that path, so closed == 2.
    //
    // The three counters are packed into one exit code: 100 + 50 + 10 = 160. The
    // multipliers are kept small deliberately — a process exit status is 8 bits,
    // so any total of 256 or more would wrap and silently mis-report the result.
    return (closed * 50) + (score * 10) + ticks
}
