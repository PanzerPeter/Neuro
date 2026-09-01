// Showcase — windowed sensor readings behind one slice signature.
//
// Integration example combining borrowed slices with features that shipped
// before them:
//   `&[T]` / `&mut [T]` slices over both a fixed-size array and a `Vec<T>` ·
//   `.slice(range)` sub-ranges · struct with `impl` methods (`&self`) ·
//   `Option<T>` returned from a fallible reader and unwrapped with `match` ·
//   `for (i, x) in xs.enumerate()` · string interpolation with the format
//   mini-language · `as` casts.
//
// The point of the slice type is that `Window::over` is written once and reads
// a raw array, a window into that array, and a heap-backed `Vec` alike — none of
// which copies a single element.

struct Window {
    count: u64,
    total: i32,
    peak: i32
}

impl Window {
    // One pass over a borrowed run. The caller keeps ownership of the elements.
    func over(samples: &[i32]) -> Window {
        if samples.len() == 0 {
            return Window { count: 0, total: 0, peak: 0 }
        }
        mut total: i32 = 0
        mut peak: i32 = samples[0]
        for s in samples {
            total = total + s
            if s > peak { peak = s }
        }
        Window { count: samples.len(), total: total, peak: peak }
    }

    func mean(&self) -> f64 {
        if self.count == 0 { return 0.0 }
        (self.total as f64) / (self.count as f64)
    }

    // `None` is the honest answer for an empty window, rather than a sentinel
    // the caller has to know about.
    func headroom(&self, ceiling: i32) -> Option<i32> {
        if self.count == 0 { return Option::None }
        Option::Some(ceiling - self.peak)
    }
}

// A `&mut [T]` writes through to whatever buffer it borrows, so the caller's
// array is clamped in place with no copy back.
func clamp_each(samples: &mut [i32], ceiling: i32) -> u64 {
    mut clamped: u64 = 0
    mut i: u64 = 0
    while i < samples.len() {
        if samples[i] > ceiling {
            samples[i] = ceiling
            clamped = clamped + 1
        }
        i = i + 1
    }
    clamped
}

func report(label: string, w: &Window) {
    println("{label:<9} n={w.count}  total={w.total:>4}  peak={w.peak:>3}  mean={w.mean():.2}")
}

func main() -> i32 {
    mut readings: [i32; 6] = [12, 45, 7, 91, 23, 60]

    // The whole array, then a sub-range of it — same signature, no copy.
    val all = Window::over(&readings)
    report("all", &all)
    val midday = Window::over(readings.slice(2..5))
    report("midday", &midday)

    for (hour, value) in readings.slice(0..3).enumerate() {
        println("  hour {hour}: {value:>3}")
    }

    val ceiling: i32 = 50
    val clamped = clamp_each(&mut readings, ceiling)
    println("clamped {clamped} reading(s) to {ceiling}")
    val capped = Window::over(&readings)
    report("capped", &capped)

    // The same reader over a heap-backed source.
    mut spares: Vec<i32> = Vec::new()
    spares.push(30)
    spares.push(18)
    spares.push(44)
    val backup = Window::over(&spares)
    report("spares", &backup)

    val room = match capped.headroom(ceiling + 10) {
        Option::Some(r) => r,
        Option::None => 0
    }
    println("headroom below {ceiling + 10} = {room}")

    // capped.total = 12 + 45 + 7 + 50 + 23 + 50 = 187; backup.total = 92;
    // room = 60 - 50 = 10. 187 - 92 - 10 = 85.
    return capped.total - backup.total - room
}
