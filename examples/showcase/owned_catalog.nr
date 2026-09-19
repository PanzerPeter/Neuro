// Showcase: a catalog whose collections own the text they hold.
//
// The sub-phase closer for the value model. Everything 2E built is in one program:
//   move semantics and the by-value ABI · a consuming `self` receiver ·
//   non-`Copy` values inside aggregates · drop for a reassigned binding ·
//   drop for a value held in a struct field · release of an anonymous heap
//   string · place expressions (`self.bytes += n`, `names[i] = ...`) ·
//   the owned string a holder stores · and the collection slots below.
//
// The rule this program leans on: a collection never shares a buffer across its
// own boundary. `push` and `insert` copy the bytes INTO a slot the collection
// owns, so the argument is read rather than taken and the binding it came from
// is still usable; `v[i]`, `for x in v`, `m.get(k)` and `m.keys()` copy back
// OUT, so what a read hands over stays valid after the collection is gone. Only
// `pop` transfers, because the slot goes with it.
//
// That is what makes `first_name` below sound. It returns a read of a `Vec` its
// own caller destroys on the next line; under an aliasing read the returned
// value would point into a freed buffer, and the comparison after it would be
// reading someone else's memory.
//
// The counts are what the program asserts, and they only come out right if each
// buffer was intact when it was read. The loop keeps the heap flat only if each
// one was also released exactly once.

const ROUNDS: i32 = 200

struct Catalog {
    label: string,
    bytes: u64,
    items: i32
}

impl Catalog {
    // A place expression on a field: the compound assignment reads and writes
    // the same storage.
    func record(&mut self, n: u64) {
        self.bytes += n
        self.items += 1
    }

    // A consuming receiver: the catalog is gone after this, and its `label`
    // field goes with it.
    func into_report(self) -> string {
        return "{self.label}: {self.items}"
    }
}

// Allocates on every return path, so the caller owns what it gets back.
func tagged(kind: &string, id: i32) -> string {
    return kind + "-" + "{id}"
}

// The read outlives the collection: the `Vec` this borrows from is destroyed by
// the caller straight after, and the returned buffer is the reader's own.
func first_name(names: &Vec<string>) -> string {
    return names[0]
}

func main() -> i32 {
    val kind: string = "sku"
    mut catalog = Catalog { label: kind + "-catalog", bytes: 0, items: 0 }

    // The three collections, each holding text they own rather than borrow.
    mut names: Vec<string> = Vec::new()
    mut index: HashMap<string, i32> = HashMap::new()
    mut notes: BTreeMap<string, string> = BTreeMap::new()

    val bolt: string = kind + "-bolt"
    names.push(bolt)
    names.push(tagged(&kind, 2))
    names.push("plain")

    // `push` read `bolt` instead of taking it, so the binding is still live and
    // still released by its own scope.
    if bolt != "sku-bolt" {
        return 91
    }
    if names[0] != bolt {
        return 92
    }

    index.insert(bolt, 0)
    index.insert(tagged(&kind, 2), 1)
    notes.insert("first", kind + "-note")
    // An insertion onto a key already present replaces the value and releases
    // the one it displaced.
    notes.insert("first", kind + "-revised")

    for name in names {
        catalog.record(name.len())
    }

    // A place expression on a collection slot: the element it displaces is
    // released, and the replacement is owned by the slot.
    names[2] = tagged(&kind, 3)
    catalog.record(names[2].len())

    // The fallible readers hand their payload out inside an `Option`; the arm
    // that binds it owns what it took.
    match index.get(bolt) {
        Option::Some(at) => { catalog.record(at as u64) }
        Option::None => { return 93 }
    }
    match notes.get("first") {
        Option::Some(note) => { catalog.record(note.len()) }
        Option::None => { return 94 }
    }

    // `keys()` is a `Vec` of its own, with copies of the map's keys in it.
    mut key_bytes: u64 = 0
    for key in notes.keys() {
        key_bytes = key_bytes + key.len()
    }
    if key_bytes != 5 {
        return 95
    }

    // The loop is where a missed release would show as a growing heap and a
    // double release as an abort. Every shape the boundary has is in it.
    mut i: i32 = 0
    while i < ROUNDS {
        mut round: Vec<string> = Vec::new()
        round.push(tagged(&kind, i))
        round.push(bolt)
        catalog.record(round[0].len())

        mut seen: HashMap<string, string> = HashMap::new()
        seen.insert(tagged(&kind, i), kind + "-seen")
        match seen.get(tagged(&kind, i)) {
            Option::Some(hit) => { catalog.record(hit.len()) }
            Option::None => { return 96 }
        }

        match round.pop() {
            Option::Some(last) => { catalog.record(last.len()) }
            Option::None => { return 97 }
        }
        round.clear()
        i = i + 1
    }

    // A builder fed from collection reads, to show the copies are ordinary
    // owned strings once they leave the slot.
    mut report = String::new()
    report.push_str(first_name(&names))
    report.push_str(";")
    report.push_str(names[1])
    val transcript: string = report.to_string()
    if transcript != "sku-bolt;sku-2" {
        return 98
    }

    // Reassigning a tracked binding releases what it displaced.
    mut current: string = kind + "-current"
    current = kind + "-final"
    catalog.record(current.len())

    val items = catalog.items
    val bytes = catalog.bytes
    // The consuming receiver takes the catalog and its field's buffer with it.
    val summary = catalog.into_report()

    println("items:   {items}")
    println("bytes:   {bytes}")
    println("summary: {summary}")
    println("report:  {transcript}")

    return (bytes % 251) as i32
}
