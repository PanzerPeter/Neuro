// Showcase: a run transcript assembled in one growable `StringBuilder`. Integration
// example: `StringBuilder` with `push_str`, `Vec` iteration, a struct with `&self` and
// consuming `self` methods, an enum with a payload, the format specs `:>W`, `:.N` and
// `:+d`, and `&mut StringBuilder` parameters. Each event appends to one buffer that grows
// in place. Concatenating with `+` would recopy the whole transcript for every event. The
// exit code is the byte length of the transcript.

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

// Appending through a `&mut StringBuilder` keeps the caller's one buffer: the borrow
// writes into the original allocation rather than returning a new string.
func append_event(out: &mut StringBuilder, event: Event) {
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

    mut transcript = StringBuilder::new()
    for event in &events {
        append_event(&mut transcript, *event)
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
