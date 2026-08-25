// Showcase — a formatted status report built from live readings.
//
// Integration example combining string interpolation with the features that
// produce the values it renders:
//   struct + impl methods (&self) · enum with a payload + match · fixed-size
//   array + for-in loop · f64 math and `as` casts · string concatenation ·
//   string interpolation with the format mini-language (:.N, :x, :>W, :<W, :+d).
//
// Every rendered line is checked against the exact text it should produce, and
// `main` returns the total width of the report's aligned column as its exit
// code — a number that only comes out right if every field padded correctly.

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
// A `[string; N]` is not available yet — arrays hold Copy elements only.
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

    mut matched: i32 = 0
    mut index: i32 = 0
    for sensor in sensors {
        val rendered = sensor.line() + " | " + describe(sensor.level())
        matched = matched + check(rendered, expected_line(index))
        index += 1
    }

    // A summary line pads the label into a fixed column so the report lines up.
    // A hole may not contain a `"` literal, so the label is bound first.
    val label = "checked"
    val summary = "{label:>10}: {matched} of {index}"
    matched = matched + check(summary, "   checked: 3 of 3")

    if matched != 4 {
        return 0
    }

    // The report's aligned column is 10 wide; report it scaled by the number of
    // sensors that rendered exactly as expected.
    return 10 * index + matched
}
