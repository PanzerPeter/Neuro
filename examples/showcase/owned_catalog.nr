// Showcase: a catalog whose collections own the text they hold. Integration example: move
// semantics, a consuming `self` receiver, drop of a reassigned binding and of a struct
// field, place expressions such as `self.bytes += n` and `names[i] = ...`, and `Vec`,
// `HashMap` and `BTreeMap` slots. `push` and `insert` move their argument into a slot,
// `v[i]` and `m.get(k)` read in place as borrows, and `pop` hands the value over. To keep a
// value and also store it, `.clone()` it first. `first_name` returns a copy, so its result
// is independent of the collection. The counts only come out right if each buffer was
// intact when read and released exactly once.

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

// A slot is read in place, so a value that must outlive the read is a copy of it.
func first_name(names: &Vec<string>) -> string {
    return names[0].clone()
}

func main() -> i32 {
    val kind: string = "sku"
    mut catalog = Catalog { label: kind + "-catalog", bytes: 0, items: 0 }

    // The three collections, each holding text they own rather than borrow.
    mut names: Vec<string> = Vec::new()
    mut index: HashMap<string, i32> = HashMap::new()
    mut notes: BTreeMap<string, string> = BTreeMap::new()

    val bolt: string = kind + "-bolt"
    names.push(bolt.clone())
    names.push(tagged(&kind, 2))
    names.push("plain")

    // `push` took a copy of `bolt`, so the binding is still live and still
    // released by its own scope, and the slot reads back the same text in place.
    if bolt != "sku-bolt" {
        return 91
    }
    if names[0] != bolt {
        return 92
    }

    // This insertion moves `bolt` itself into the map: nothing reads it after.
    index.insert(bolt, 0)
    index.insert(tagged(&kind, 2), 1)
    notes.insert("first", kind + "-note")
    // An insertion onto a key already present replaces the value and releases
    // the one it displaced.
    notes.insert("first", kind + "-revised")

    // A borrowed head reads each element where it lies.
    for name in &names {
        catalog.record(name.len())
    }

    // A place expression on a collection slot: the element it displaces is
    // released, and the replacement is owned by the slot.
    names[2] = tagged(&kind, 3)
    catalog.record(names[2].len())

    // `get` lends the slot inside an `Option`: the arm reads through the borrow.
    match index.get("sku-bolt") {
        Option::Some(at) => { catalog.record(*at as u64) }
        Option::None => { return 93 }
    }
    match notes.get("first") {
        Option::Some(note) => { catalog.record(note.len()) }
        Option::None => { return 94 }
    }

    // `keys()` lends each key in turn; nothing is copied out of the map.
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
        round.push(names[0].clone())
        catalog.record(round[0].len())

        mut seen: HashMap<string, string> = HashMap::new()
        seen.insert(tagged(&kind, i), kind + "-seen")
        match seen.get(tagged(&kind, i)) {
            Option::Some(hit) => { catalog.record(hit.len()) }
            Option::None => { return 96 }
        }

        // `pop` hands the slot's own buffer to the arm, which releases it.
        match round.pop() {
            Option::Some(last) => { catalog.record(last.len()) }
            Option::None => { return 97 }
        }
        round.clear()
        i = i + 1
    }

    // A builder fed from a copy and from a slot read in place.
    mut report = StringBuilder::new()
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
