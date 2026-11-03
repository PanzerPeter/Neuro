// Showcase: a formatted status report built from live readings. Integration example: a
// struct with `&self` methods, an enum with a payload matched, a fixed array walked by
// `for`-in, `f64` math and `as` casts, string concatenation, the format specs `:.N`, `:x`,
// `:>W`, `:<W` and `:+d`, and `println`. Each line is checked against the exact text it
// should produce, then printed. The exit code is the width of the aligned column, which is
// right only if every field padded correctly.

enum Level {
    Nominal,
    Warning(i32),
    Critical(i32)
}

@derive(Copy, Clone)
struct Sensor {
    id: i32,
    reading: f64
}

impl Sensor {
    // A reading is nominal below 50, a warning below 80, critical above it.
    // The payload carries how far past the threshold the reading sits.
    func level(&self) -> Level {
        val whole = self.reading as i32
        if whole < 50 {
            return Level::Nominal
        }
        if whole < 80 {
            return Level::Warning(whole - 50)
        }
        return Level::Critical(whole - 80)
    }

    // `id` is rendered as a fixed-width hex tag, the reading to two decimals.
    func line(&self) -> string {
        return "sensor 0x{self.id:04x} @ {self.reading:.2}"
    }
}

func describe(level: Level) -> string {
    match level {
        Level::Nominal => "nominal",
        Level::Warning(over) => "warning {over:+d} over",
        Level::Critical(over) => "critical {over:+d} over"
    }
}

// The report line each sensor is expected to produce, keyed by position.
// A `[string; N]` is not available yet: arrays hold Copy elements only.
func expected_line(index: i32) -> string {
    match index {
        0 => "sensor 0x0011 @ 42.50 | nominal",
        1 => "sensor 0x0102 @ 63.25 | warning +13 over",
        _ => "sensor 0x0fff @ 91.00 | critical +11 over"
    }
}

func check(actual: string, expected: string) -> i32 {
    if actual == expected {
        return 1
    }
    return 0
}

func main() -> i32 {
    val sensors: [Sensor; 3] = [
        Sensor { id: 17, reading: 42.5 },
        Sensor { id: 258, reading: 63.25 },
        Sensor { id: 4095, reading: 91.0 }
    ]

    println("--- sensor report ---")

    mut matched: i32 = 0
    mut index: i32 = 0
    for sensor in sensors {
        val rendered = sensor.line() + " | " + describe(sensor.level())
        println(rendered)
        matched = matched + check(rendered, expected_line(index))
        index += 1
    }

    // A summary line pads the label into a fixed column so the report lines up.
    // A hole may not contain a `"` literal, so the label is bound first.
    val label = "checked"
    val summary = "{label:>10}: {matched} of {index}"
    println(summary)
    matched = matched + check(summary, "   checked: 3 of 3")

    if matched != 4 {
        return 0
    }

    // The report's aligned column is 10 wide; report it scaled by the number of
    // sensors that rendered exactly as expected.
    return 10 * index + matched
}
