// Showcase: text that is built, read once, and thrown away.
//
// Integration example combining the release of anonymous heap strings with the
// features that produce them:
//   `[T; N]` arrays + `for x in a` · `String` builder + `push_str` · string
//   interpolation with the format mini-language (:>W) · `pool { }` · struct +
//   `impl` methods · `&string` parameters · `.slice(range)`.
//
// Every string below is owned by nobody: a `+` chain, an interpolation, and a
// `String::to_string` copy each allocate a buffer that is read by one consumer
// and then unreachable. None of them is ever bound to a name, so no scope exit
// can release them: they are released where they are consumed instead. Run the
// loop with a large `ROUNDS` and the process's heap stays flat.
//
// The counts are what the program asserts: they only come out right if every
// buffer was still intact when it was read, which a release emitted one step too
// early would break.

const ROUNDS: i32 = 200

struct Column {
    title: string
}

impl Column {
    // The heading is a fresh buffer every call, padded into the column. Nothing
    // holds it: the caller measures it and drops it on the floor.
    func heading(&self) -> string {
        return "{self.title:>6}|"
    }

    func matches(&self, probe: &string) -> bool {
        return self.title == probe
    }
}

// Renders one row into the caller's buffer. The argument to `push_str` is a
// concatenation built for this call alone, and the append copies its bytes in,
// so the buffer it allocated has no reader left the moment `push_str` returns.
func append_row(out: &mut String, label: &string, value: i32) {
    out.push_str(label + "=" + "{value}" + ";")
}

func main() -> i32 {
    val columns: [Column; 3] = [
        Column { title: "alpha" },
        Column { title: "beta" },
        Column { title: "gamma" }
    ]

    // Each heading is an interpolation's buffer, measured and then unreachable.
    mut header_bytes: u64 = 0
    for column in &columns {
        header_bytes = header_bytes + column.heading().len()
    }
    if header_bytes != 21 {
        return 91
    }

    // Comparison reads both operands' bytes and keeps neither, so an operand
    // built for the comparison is dead as soon as it answers. A chain of `+`
    // allocates once per operator, and all but the last result are anonymous.
    val wanted: string = "beta"
    mut named: i32 = 0
    for column in &columns {
        if column.matches(&wanted) {
            named = named + 1
        }
        if "[" + column.title + "]" == "[gamma]" {
            named = named + 1
        }
    }
    if named != 2 {
        return 92
    }

    // The same shapes under a `pool`: the concatenations draw their buffers from
    // the arena, the release leaves an arena pointer alone, and the block's
    // closing brace reclaims the whole region in one step.
    mut pooled: u64 = 0
    pool {
        mut i: i32 = 0
        while i < ROUNDS {
            pooled = pooled + ("row " + "{i}" + " ok").len()
            i = i + 1
        }
    }
    if pooled != 1890 {
        return 93
    }

    // Off the arena, the same loop is one allocation and one release per round.
    val label: string = "n"
    mut rows = String::new()
    mut i: i32 = 0
    while i < ROUNDS {
        append_row(&mut rows, &label, i)
        i = i + 1
    }
    val transcript: string = rows.to_string()
    if transcript.len() != rows.len() {
        return 94
    }
    if transcript.slice(0..12) != "n=0;n=1;n=2;" {
        return 95
    }

    println("header bytes: {header_bytes}")
    println("pooled bytes: {pooled}")
    println("rows: {i}")
    return (transcript.len() % 251) as i32
}
