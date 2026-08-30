// Showcase — a shift report long enough to exercise buffered standard output.
//
// Integration example combining the features that feed a printing loop:
//   struct + `@derive(Copy, Clone)` + `impl` methods (`&self`) · `Vec<T>` +
//   `for x in v` · the growable `String` + `push_str` · string interpolation
//   with the format mini-language (:>W, :.N) · `while` loop · `%` and `as`.
//
// The point of this example is the volume. `println` copies its bytes into a
// page-sized buffer and drains it when it fills, so the 240 lines below cross
// that boundary repeatedly and their order across the drains is what the golden
// output pins down. The banner is deliberately larger than the buffer will ever
// be: a string that cannot fit goes straight to the descriptor, and it must not
// overtake the lines already waiting in front of it.
//
// The exit code is the number of readings at or above the peak threshold, which
// only comes out right if every reading was built and classified correctly.

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
    for reading in readings {
        if reading.at_peak() {
            peaks = peaks + 1
        }
        println(reading.render())
    }

    // One string larger than the output buffer, assembled in a single growable
    // allocation rather than by re-concatenating a longer string each pass.
    mut banner = String::new()
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
