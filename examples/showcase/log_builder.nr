// Showcase — a run transcript assembled in a single growable buffer.
//
// Integration example combining the growable `String` with the features that
// feed it:
//   `Vec<T>` + `for x in v` · struct + `impl` methods (`&self`) · enum with a
//   payload + `match` · string interpolation with the format mini-language
//   (:>W, :.N, :+d) · `&mut String` parameters · `@derive(Copy, Clone)`.
//
// The point of the buffer is the loop: each event appends its rendered line to
// one allocation that grows in place. Concatenating with `+` would allocate and
// recopy the whole transcript once per event instead.
//
// The finished transcript is checked against the exact text it should produce,
// and `main` returns the transcript's byte length as its exit code — a number
// that only comes out right if every field rendered and padded correctly.

@derive(Copy, Clone)
struct Event {
    code: i32,
    load: f64
}

enum Severity {
    Info,
    Warn(i32),
    Fail(i32)
}

impl Event {
    // Load below 50 is routine, below 80 a warning; the payload carries how far
    // past the threshold the reading sits.
    func severity(&self) -> Severity {
        val whole = self.load as i32
        if whole < 50 {
            return Severity::Info
        }
        if whole < 80 {
            return Severity::Warn(whole - 50)
        }
        return Severity::Fail(whole - 80)
    }

    func render(&self) -> string {
        return "{self.code:>4} {self.load:.1}"
    }
}

func label(severity: Severity) -> string {
    match severity {
        Severity::Info => "info",
        Severity::Warn(over) => "warn {over:+d}",
        Severity::Fail(over) => "fail {over:+d}"
    }
}

// Appending through a `&mut String` keeps the caller's one buffer: the borrow
// writes into the original allocation rather than returning a new string.
func append_event(out: &mut String, event: Event) {
    out.push_str(event.render())
    out.push_str(" ")
    out.push_str(label(event.severity()))
    out.push_str("\n")
}

func main() -> i32 {
    mut events: Vec<Event> = Vec::new()
    events.push(Event { code: 7, load: 12.5 })
    events.push(Event { code: 108, load: 63.0 })
    events.push(Event { code: 4095, load: 91.25 })

    mut transcript = String::new()
    for event in events {
        append_event(&mut transcript, event)
    }

    // A trailing summary line pads its label into a fixed column. A hole may not
    // contain a `"` literal, so the label is bound first.
    val counted: u64 = events.len()
    val header = "events"
    transcript.push_str("{header:>8}: {counted}")

    val text: string = transcript.to_string()
    val expected: string = "   7 12.5 info\n 108 63.0 warn +13\n4095 91.2 fail +11\n  events: 3"
    if text != expected {
        return 91
    }

    // The builder is reusable: clearing keeps the buffer and refills it.
    transcript.clear()
    if transcript.len() != 0 {
        return 92
    }

    return text.len() as i32
}
