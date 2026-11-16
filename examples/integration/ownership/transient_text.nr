// Text that is built, read once, and thrown away. Integration example: `[T; N]`
// arrays iterated by value, `StringBuilder` with `push_str`, interpolation with a width,
// `pool { }`, a struct with `impl` methods, `&string` parameters, and `.slice(range)`. Each
// string is owned by nobody: a `+` chain, an interpolation and a `StringBuilder::to_string`
// copy each allocate a buffer that one consumer reads before it becomes unreachable.
// Nothing binds these to a name, so they are released where they are consumed, or, when a
// view is taken of one, where the statement reading the view ends. A struct handed over by
// value releases the text it holds where its new owner ends. With a large `ROUNDS` the heap
// stays flat.

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
func append_row(out: &mut StringBuilder, label: &string, value: i32) {
    out.push_str(label + "=" + "{value}" + ";")
}

// Takes the column by value: its title is released when this call returns.
func title_bytes(column: Column) -> u64 {
    return column.title.len()
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
        header_bytes += column.heading().len()
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
            named += 1
        }
        if "[" + column.title + "]" == "[gamma]" {
            named += 1
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
            pooled += ("row " + "{i}" + " ok").len()
            i += 1
        }
    }
    if pooled != 1890 {
        return 93
    }

    // Off the arena, the same loop is one allocation and one release per round.
    val label: string = "n"
    mut rows = StringBuilder::new()
    mut i: i32 = 0
    while i < ROUNDS {
        append_row(&mut rows, &label, i)
        i += 1
    }
    val transcript: string = rows.to_string()
    if transcript.len() != rows.len() {
        return 94
    }
    if transcript.slice(0..12) != "n=0;n=1;n=2;" {
        return 95
    }

    // A view of a buffer nothing binds keeps that buffer to the end of its statement,
    // and a `for` over one keeps it for the whole loop. The checker refuses to bind such
    // a view to a name, which would outlive the buffer.
    mut viewed: u64 = 0
    mut k: i32 = 0
    while k < ROUNDS {
        viewed += ("row " + "{k}").slice(0..3).len()
        for c in (label + "{k}").chars() {
            if c == 'n' {
                viewed += 1
            }
        }
        k += 1
    }
    if viewed != 800 {
        return 96
    }
    if title_bytes(Column { title: "del" + "ta" }) != 5 {
        return 97
    }

    println("header bytes: {header_bytes}")
    println("viewed bytes: {viewed}")
    println("pooled bytes: {pooled}")
    println("rows: {i}")
    return (transcript.len() % 251) as i32
}
