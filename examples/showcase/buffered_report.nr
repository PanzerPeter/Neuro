// Showcase: a shift report long enough to exercise buffered standard output. Integration
// example: a struct with `&self` methods, `Vec<T>` iteration, a `StringBuilder`,
// interpolation with width and precision, `while`, `%` and `as`. `println` buffers its
// bytes and drains a page-sized buffer when it fills. The report crosses that boundary
// repeatedly, and the golden output pins the order across drains. The banner is larger than
// the buffer, so it goes straight to the descriptor and must not overtake the lines already
// waiting. The exit code is the number of readings at or above the peak threshold.

@derive(Copy, Clone)
struct Reading {
    minute: i32,
    load: f64
}

impl Reading {
    func at_peak(&self) -> bool {
        return self.load >= 90.0
    }

    func render(&self) -> string {
        return "minute {self.minute:>3} | load {self.load:.2}"
    }
}

func main() -> i32 {
    // A four-hour shift sampled once a minute, cycling through the load band.
    mut readings: Vec<Reading> = Vec::new()
    mut minute: i32 = 0
    while minute < 240 {
        val load = 40.0 + ((minute % 61) as f64)
        readings.push(Reading { minute: minute, load: load })
        minute = minute + 1
    }

    mut peaks: i32 = 0
    for reading in &readings {
        if reading.at_peak() {
            peaks = peaks + 1
        }
        println(reading.render())
    }

    // One string larger than the output buffer, assembled in a single growable
    // allocation rather than by re-concatenating a longer string each pass.
    mut banner = StringBuilder::new()
    mut row: i32 = 0
    while row < 160 {
        banner.push_str("[{row:>3}] shift block filler row\n")
        row = row + 1
    }
    print(banner.to_string())

    val total: u64 = readings.len()
    println("readings {total} | peaks {peaks}")
    return peaks
}
