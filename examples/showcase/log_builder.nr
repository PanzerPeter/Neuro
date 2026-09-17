// Showcase: a run transcript assembled in a single growable buffer.
//
// Integration example combining the growable `String` with the features that
// feed it:
//   `Vec<T>` + `for x in v` · struct + `impl` methods (`&self` and a consuming
//   `self`) · enum with a payload + `match` · string interpolation with the
//   format mini-language (:>W, :.N, :+d) · `&mut String` parameters ·
//   `@derive(Copy, Clone)`.
//
// The point of the buffer is the loop: each event appends its rendered line to
// one allocation that grows in place. Concatenating with `+` would allocate and
// recopy the whole transcript once per event instead.
//
// The finished transcript is checked against the exact text it should produce,
// and `main` returns the transcript's byte length as its exit code: a number
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

// A finished transcript. `into_text` takes `self` by value, so handing the
// transcript over is the only way to read its text out: the method owns the
// receiver and moves the `string` field straight out of it, and the caller
// cannot name `finished` again afterwards.
struct Transcript {
    text: string,
    events: u64
}

impl Transcript {
    func event_count(&self) -> u64 {
        self.events
    }

    func into_text(self) -> string {
        self.text
    }
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

    val finished = Transcript {
        text: transcript.to_string(),
        events: counted
    }
    val rendered: u64 = finished.event_count()   // borrows: `finished` survives
    val text: string = finished.into_text()      // consumes: `finished` is gone
    // val again = finished.event_count()        // <- rejected: use of moved value
    println(text)

    val expected: string = "   7 12.5 info\n 108 63.0 warn +13\n4095 91.2 fail +11\n  events: 3"
    if text != expected {
        println("transcript did not match the expected text")
        return 91
    }

    // The builder is reusable: clearing keeps the buffer and refills it.
    transcript.clear()
    if transcript.len() != 0 {
        return 92
    }

    if rendered != counted {
        return 93
    }

    val bytes = text.len() as i32
    println("transcript bytes: {bytes}")
    return bytes
}
