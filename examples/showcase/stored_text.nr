// Showcase: text that is built, STORED, and released by whoever stored it.
//
// Integration example combining the release of an owned string a holder stores
// with the features that put it there:
//   struct + `impl` methods · `[T; N]` arrays + `for x in &a` · tuples ·
//   place expressions (`self.total += n`, `entry.label = ...`) · `String`
//   builder + `push_str` · string interpolation · `&string` parameters.
//
// The sibling program `transient_text.nr` covers the other half: a buffer no
// position ever stores, released at the consumer that reads it. Here every
// buffer reaches a position that outlives the expression that built it, so the
// holder is what releases it:
//   * a struct field and an array or tuple element, when the store into it
//     provably allocated — and NOT when it points at a `.rodata` literal;
//   * a function's return value, released by the caller that took it;
//   * an argument handed to a parameter the callee only reads, released at the
//     call it was built for.
//
// The counts are what the program asserts. They only come out right if every
// buffer was intact when it was read, which a release emitted one step early
// would break, and the loop keeps the heap flat only if each one was released
// exactly once.

const ROUNDS: i32 = 300

struct Entry {
    label: string,
    weight: i32
}

struct Ledger {
    total: u64,
    rows: i32
}

impl Ledger {
    // A place expression on a field: the compound assignment reads and writes
    // the same storage.
    func record(&mut self, n: u64) {
        self.total += n
        self.rows += 1
    }
}

// Allocates on every return path, so the caller owns what it gets back.
func tagged(kind: &string, id: i32) -> string {
    return kind + "-" + "{id}"
}

// Reads its argument and keeps none of it, so a buffer built for this call is
// dead the moment it returns.
func width(text: string) -> u64 {
    return text.len()
}

// Stores its argument in the value it hands back, so the buffer outlives the
// call and the caller must leave it alone.
func wrap(text: string, weight: i32) -> Entry {
    return Entry { label: text, weight: weight }
}

func main() -> i32 {
    val kind: string = "row"

    // A field, an array element and a tuple element each take a fresh buffer and
    // release it when their holder is destroyed. The literal in the same shape
    // takes on nothing: freeing `.rodata` would abort rather than leak.
    mut ledger = Ledger { total: 0, rows: 0 }
    mut i: i32 = 0
    while i < ROUNDS {
        val entries: [Entry; 2] = [
            Entry { label: kind + "-a", weight: 1 },
            Entry { label: "static", weight: 2 }
        ]
        for entry in &entries {
            ledger.record(entry.label.len())
        }

        val pair: (string, i32) = (kind + "-pair", i)
        ledger.record(pair.0.len())

        // Reassignment through a field place: the displaced buffer goes, and the
        // replacement is owned only because it allocated.
        mut current = Entry { label: kind + "-first", weight: 0 }
        current.label = kind + "-second"
        ledger.record(current.label.len())

        // The callee allocated on every path, so this binding owns the result.
        val label: string = tagged(&kind, i)
        ledger.record(label.len())

        // Built for the call and released at it.
        ledger.record(width(kind + "-probe"))

        i = i + 1
    }

    // The boundary, once and deliberately outside the loop. `wrap` STORES its
    // argument in the value it hands back, so the buffer outlives the call and
    // the caller must not release it there; and a holder a call built proves
    // nothing about who owns its positions, so the binding does not release it
    // either. One buffer is leaked rather than one dangling pointer handed out,
    // which is the direction every unproven case here errs in.
    val kept = wrap(kind + "-kept", 3)
    ledger.record(kept.label.len())

    // Six positions per round: the two array labels (5 and 6 bytes), the tuple
    // element (8), the reassigned field (10) and the call's argument (9) are
    // fixed at 38; the tagged label grows with the round's digits, for 1990 over
    // the range. The one `wrap` afterwards adds its row and its 8 bytes.
    if ledger.rows != ROUNDS * 6 + 1 {
        return 91
    }
    if ledger.total != 13398 {
        return 92
    }

    // The same shapes feeding a builder, to show the stored buffers and the
    // transient ones coexist.
    mut report = String::new()
    report.push_str(tagged(&kind, 1))
    report.push_str(";")
    report.push_str(tagged(&kind, 2))
    val transcript: string = report.to_string()
    if transcript != "row-1;row-2" {
        return 93
    }

    println("rows: {ledger.rows}")
    println("bytes: {ledger.total}")
    println("transcript: {transcript}")
    return (ledger.total % 251) as i32
}
