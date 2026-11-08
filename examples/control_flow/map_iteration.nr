// A map is a `for` head in all three receiver modes. `for (k, v) in &m` lends each entry,
// `for (k, v) in &mut m` lends the value mutably (never the key, which places the entry),
// and `for (k, v) in m` moves the entries out and consumes the map. `m.keys()` and
// `m.values()` lend one half of each entry. A `BTreeMap` walks in ascending key order; a
// `HashMap` promises no order, so this program only sums what it reads from one.

// A key type parameter needs the map's key traits and nothing else: the key moves into
// its slot like a value, so a `string` instance works as well as an `i32` one.
func tally<K: Hashable + PartialEq>(keys: Vec<K>) -> HashMap<K, i32> {
    mut counts: HashMap<K, i32> = HashMap::new()
    for key in keys {
        val seen = counts.remove(key) ?? 0
        counts.insert(key, seen + 1)
    }
    counts
}

func main() -> i32 {
    mut stock: BTreeMap<string, i32> = BTreeMap::new()
    stock.insert("pears", 4)
    stock.insert("apples", 7)
    stock.insert("figs", 2)

    for (fruit, count) in &stock {
        println("{fruit}: {count}")
    }

    // Restock in place through the mutable head.
    for (_, count) in &mut stock {
        *count += 10
    }

    mut names = ""
    for fruit in stock.keys() {
        names = names + "{fruit} "
    }
    println("keys in order: {names}")

    mut total = 0
    for count in stock.values().filter(|c| c > 12) {
        total += *count
    }
    println("restocked over 12: {total}")

    mut words: Vec<string> = Vec::new()
    words.push("fig")
    words.push("pear")
    words.push("fig")
    val counts = tally(words)
    mut figs = 0
    for (word, n) in counts {
        if word == "fig" {
            figs = n
        }
    }
    println("fig counted {figs} times")

    // Consuming the ordered map: the first entry is the smallest key.
    mut first = ""
    for (fruit, count) in stock {
        first = fruit
        break
    }
    println("first fruit: {first}")
    total + figs
}
