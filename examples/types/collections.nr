// Standard collections: Vec<T>, HashMap<K, V>, BTreeMap<K, V>.
//
// All three are heap-backed library types. They are not Copy — assignment moves
// them — and each frees its buffer when its owner leaves scope. The fallible
// readers (`pop`, `get`) return `Option<T>`, so a miss is a value to match on
// rather than a trap.

func main() -> i32 {
    // --- Vec<T>: growable, indexable, iterable ---------------------------
    mut counts: Vec<i32> = Vec::new()
    counts.push(3)
    counts.push(5)
    counts.push(7)
    counts[0] = 4

    mut sum: i32 = 0
    for c in counts {
        sum = sum + c
    }
    // sum == 16

    println("Vec  after push/index -> for-in sum {sum}")

    match counts.pop() {
        Option::Some(last) => { sum = sum + last }   // + 7 -> 23
        Option::None => { sum = sum - 100 }
    }
    println("Vec  pop() Some       -> {sum}")

    // `get` is the checked counterpart to `counts[i]`, which panics out of range.
    match counts.get(9u64) {
        Option::Some(_) => { sum = sum - 100 }
        Option::None => { sum = sum + 1 }            // -> 24
    }
    println("Vec  get(9) None      -> {sum}")

    // --- HashMap<K, V>: average-O(1) lookup ------------------------------
    mut stock: HashMap<string, i32> = HashMap::new()
    stock.insert("bolt", 40)
    stock.insert("nut", 25)
    stock.insert("bolt", 42)                          // overwrites in place

    match stock.get("bolt") {
        Option::Some(n) => { sum = sum + n }          // + 42 -> 66
        Option::None => { sum = sum - 100 }
    }
    if stock.contains_key("nut") {
        sum = sum + 1                                 // -> 67
    }
    if stock.remove("nut") {
        sum = sum + 1                                 // -> 68
    }
    println("Map  get/contains/rm  -> {sum}")

    // --- BTreeMap<K, V>: key-ordered -------------------------------------
    // `keys()` returns a Vec, so an ordered map's iteration order is observable.
    mut ranks: BTreeMap<i32, i32> = BTreeMap::new()
    ranks.insert(30, 3)
    ranks.insert(10, 1)
    ranks.insert(20, 2)

    mut highest: i32 = 0
    for key in ranks.keys() {
        println("BTreeMap key          -> {key}")     // ascending: 10, 20, 30
        highest = key                                 // ascending, so 30 last
    }

    val total = sum + highest + ranks.len() as i32
    println("total                 -> {total}")
    total                                             // 68 + 30 + 3 = 101
}
