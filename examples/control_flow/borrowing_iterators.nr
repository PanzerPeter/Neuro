// `.iter()` on an array, a slice or a `Vec` is a value: the library's `Iter<T>`, yielding a
// `&T` per element. A map's `.keys()` and `.values()` are values too outside a `for` head,
// yielding `&K` and `&V`. Each borrows its container, so walking copies and consumes
// nothing, and the container cannot change while the iterator lives. Being values, they
// bind, take the adapters, pass to a generic consumer and come back from a function.

// Any stream of borrowed `i32`s.
func total<I: Iterator<Item = &i32>>(values: I) -> i32 {
    mut sum = 0
    for value in values {
        sum = sum + value
    }
    sum
}

// An iterator over the caller's elements may leave the function: what it borrows came in
// through the parameter, so it outlives the call.
func above(scores: &[i32], floor: i32) -> Filter<Iter<i32>, &i32> {
    scores.iter().filter(|s| s > floor)
}

// The same for a map: the names of an ordered map come back in ascending order.
func roster(teams: &BTreeMap<string, i32>) -> BTreeKeys<string, i32> {
    teams.keys()
}

func main() -> i32 {
    mut scores: Vec<i32> = Vec::new()
    for s in [7, 12, 3, 15, 9] {
        scores.push(s)
    }

    // Bound first and consumed later; `scores` is still usable once it is spent.
    val walk = scores.iter()
    println("all scores total {total(walk)}")
    println("scores above 8 total {total(above(&scores, 8))}")

    val bonus = scores.iter().map(|s| s * 10)
    mut bonus_total = 0
    for b in bonus {
        bonus_total = bonus_total + b
    }
    println("bonus total {bonus_total}")

    // `next` hands out one borrow at a time, and a `&mut` head picks up where it stopped.
    mut cursor = scores.iter()
    mut opening = ""
    for s in &mut cursor {
        opening = opening + "{s} "
        if *s == 12 {
            break
        }
    }
    println("opening {opening}")
    while val Option::Some(s) = cursor.next() {
        println("then {s}")
    }

    mut teams: BTreeMap<string, i32> = BTreeMap::new()
    teams.insert("owls", 3)
    teams.insert("bears", 5)
    teams.insert("cats", 1)
    for name in roster(&teams) {
        println("team {name}")
    }
    val wins = total(teams.values())
    println("wins total {wins}")

    // A hashed map has no order, so the views are summed rather than printed.
    mut lanes: HashMap<i32, i32> = HashMap::new()
    for lane in 1..=4 {
        lanes.insert(lane, lane * lane)
    }
    lanes.remove(3)
    val busy = lanes.values().filter(|v| v > 1)
    total(busy) + total(lanes.keys()) + scores.len() as i32
}
