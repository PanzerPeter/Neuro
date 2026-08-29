// An inventory ledger — the standard collections working with prior features.
//
// Combines: `Vec<T>` holding `Copy` structs, a second `Vec<string>` of names, a
// `HashMap<string, i32>` name index, a key-ordered `BTreeMap<i32, i32>` report,
// an enum + `match` classifier, an `impl` block with `&self` methods, `Option`
// matching on the fallible reads, `??` for the reads that only want a default,
// and `for`-in iteration over collections.

@derive(Copy, Clone)
struct Item {
    unit_weight: i32,
    quantity: i32
}

impl Item {
    // Total shelf weight of this line item.
    func weight(&self) -> i32 {
        self.unit_weight * self.quantity
    }

    func is_bulk(&self) -> bool {
        self.quantity >= 10
    }

    // Classify this line item by the two properties above.
    func shelf(&self) -> Shelf {
        if self.is_bulk() {
            return Shelf::Bulk
        }
        if self.weight() >= 40 {
            return Shelf::Heavy
        }
        Shelf::Light
    }
}

enum Shelf {
    Light,
    Heavy,
    Bulk
}

// Shelves carry different handling surcharges.
func surcharge(shelf: Shelf) -> i32 {
    match shelf {
        Shelf::Light => 1
        Shelf::Heavy => 5
        Shelf::Bulk => 3
    }
}

func main() -> i32 {
    // The ledger itself: line items in arrival order, plus a name index into it.
    mut items: Vec<Item> = Vec::new()
    mut names: Vec<string> = Vec::new()
    mut index: HashMap<string, i32> = HashMap::new()

    names.push("bolt")
    names.push("girder")
    names.push("washer")

    val weights: [i32; 3] = [2, 45, 1]
    val quantities: [i32; 3] = [12, 1, 4]

    mut slot: i32 = 0
    for name in names {
        val item = Item {
            unit_weight: weights[slot],
            quantity: quantities[slot]
        }
        items.push(item)
        index.insert(name, slot)
        slot = slot + 1
    }

    // Total weight, and a weight-keyed report the ordered map keeps sorted.
    mut by_weight: BTreeMap<i32, i32> = BTreeMap::new()
    mut total_weight: i32 = 0
    for item in items {
        val w = item.weight()
        total_weight = total_weight + w
        by_weight.insert(w, item.quantity)
    }
    // bolt 24, girder 45, washer 4 -> total 73

    // Look a line item up by name, through the index.
    mut girder_weight: i32 = 0
    match index.get("girder") {
        Option::Some(at) => { girder_weight = items[at].weight() }
        Option::None => { girder_weight = -1 }
    }
    // girder_weight == 45

    // A miss is an ordinary value, not a trap.
    mut missing: i32 = 0
    match index.get("rivet") {
        Option::Some(_) => { missing = -1 }
        Option::None => { missing = 1 }
    }

    // `??` reads the same fallible lookups without a `match` when the only thing the
    // absent case needs is a default. It chains right-to-left, so the second lookup
    // only runs after the first has come up empty.
    val bolt_at = index.get("bolt") ?? -1                           // 0
    val rivet_at = index.get("rivet") ?? index.get("washer") ?? -1   // 2

    // Surcharges, driven by the enum classifier.
    mut fees: i32 = 0
    for item in items {
        fees = fees + surcharge(item.shelf())
    }
    // bulk(3) + heavy(5) + light(1) = 9

    // The ordered map reports ascending weights, so the last key is the heaviest.
    mut heaviest: i32 = 0
    for w in by_weight.keys() {
        heaviest = w
    }
    // heaviest == 45

    val lines = items.len() as i32                    // 3
    val indexed = index.len() as i32                  // 3

    println("Vec<Item> lines            = {lines}")
    println("HashMap index entries      = {indexed}")
    println("total weight               = {total_weight}")
    println("index.get(girder) -> weight= {girder_weight}")
    println("index.get(rivet)  -> miss  = {missing}")
    println("?? bolt                    = {bolt_at}")
    println("?? rivet, then washer      = {rivet_at}")
    println("surcharges                 = {fees}")
    println("BTreeMap heaviest key      = {heaviest}")

    // 73 + 45 + 1 + 9 + 45 + 3 + 3 + 0 + 2 = 181
    val total = total_weight + girder_weight + missing + fees + heaviest + lines +
        indexed + bolt_at + rivet_at
    println("total                      = {total}")
    total
}
