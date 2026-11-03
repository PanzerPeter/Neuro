// Showcase: deterministic `Drop` and labeled loop exits working together. Integration
// example: `impl Drop` running at scope exit, a labeled `break` leaving a nested loop, a
// labeled `loop` as a value with `continue 'search` and `break 'search probe`, `&mut i32`
// counters held in struct fields, and `match` with range and `_` patterns. A labeled break
// is a normal scope exit, not a panic, so each guard's destructor still runs exactly once
// on that path.

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
    mut outer_closed: i32 = 0
    mut inner_closed: i32 = 0
    mut score: i32 = 0
    mut ticks: i32 = 0

    {
        // Two guards, each holding its own counter: a `&mut` is exclusive, so two
        // live guards cannot share one. Both must fire at the end of this block,
        // including on the labeled-break path below.
        val outer_log = ScanLog { closed: &mut outer_closed }
        val inner_log = ScanLog { closed: &mut inner_closed }

        val sensor = Sensor { base: 2, gain: 8 }
        val limits = [4, 12, 30]

        'sweep: for tick in 0..4 {
            for slot in 0..3 {
                ticks = ticks + 1

                val value = sensor.reading(tick)
                val grade = classify(value)

                if value > limits[slot] {
                    score = score + grade
                }

                println("tick {tick} slot {slot}: value {value} grade {grade} score {score}")

                // Critical reading ends the whole sweep, not just this inner loop.
                if grade == 2 {
                    println("critical reading -> break 'sweep")
                    break 'sweep
                }
            }
        }

        // The first tick whose reading clears the first two limits. The inner `for`
        // gives up on a tick with `continue 'search`, and `break 'search probe`
        // carries the answer out as the value of the labeled `loop`.
        mut probe: i32 = 0
        val first_clear = 'search: loop {
            for slot in 0..2 {
                if sensor.reading(probe) <= limits[slot] {
                    probe = probe + 1
                    continue 'search
                }
            }
            break 'search probe
        }
        println("first tick over limits 0 and 1 = {first_clear}")
    }

    // Readings are 2, 10, 18, 26: the last is critical, so the labeled `break`
    // fires on tick 3 after 10 inner iterations, with score 5. Both guards still
    // run their destructors on that path, so closed == 2.
    //
    // The three counters are packed into one exit code: 100 + 50 + 10 = 160. The
    // multipliers are kept small deliberately: a process exit status is 8 bits,
    // so any total of 256 or more would wrap and silently mis-report the result.
    val closed = outer_closed + inner_closed
    println("guards closed = {closed}   (destructors ran on the break path)")
    println("score         = {score}")
    println("ticks         = {ticks}")

    val code = (closed * 50) + (score * 10) + ticks
    println("packed code   = {code}")
    return code
}
