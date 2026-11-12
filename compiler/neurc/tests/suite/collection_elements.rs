// End-to-end tests for non-`Copy` collection elements: any sized value may live in a
// slot, an insertion moves it in, `v[i]` names the slot in place, the readers lend it,
// and the takers (`pop`, `remove`, `swap_remove`, a displacing `insert`) hand it over.
//
// Release is asserted the way `collection_string_ownership.rs` asserts it: the shapes
// that would leak or double-free run in a loop, a user `Drop` prints when it runs, and
// the exact drop order is the stdout.
use crate::compile_harness::{CompileTest, rejected, run, stdout_of};

#[test]
fn a_vec_of_structs_is_written_read_and_taken_through_its_slots() {
    let source = r#"
struct Person { name: string, tags: Vec<string>, age: i32 }

impl Person {
    func birthday(&mut self) { self.age += 1 }
}

func make(name: string, age: i32) -> Person {
    mut tags: Vec<string> = Vec::new()
    tags.push(name + "!")
    Person { name: name, tags: tags, age: age }
}

func main() -> i32 {
    mut people: Vec<Person> = Vec::new()
    people.push(make("ada" + "", 36))
    people.push(make("bo" + "", 20))
    people.push(Person { name: "cy", tags: Vec::new(), age: 9 })
    people[0].birthday()
    people[1].tags.push("extra")
    people[2] = make("di" + "", 41)
    mut seen = 0
    {
        val second = &people[1]
        seen = second.tags.len() as i32
    }
    val first = people.remove(0)
    val last = people.swap_remove(0)
    // ada 37, bo 20 with 2 tags, di 41 left alone
    first.age + last.age + seen * 100 + people[0].age * 1000 + people.len() as i32
}
"#;
    assert_eq!(
        run("vec_of_structs.nr", source) as u32 & 0xff,
        (37 + 20 + 200 + 41_000 + 1) & 0xff
    );
}

#[test]
fn a_user_drop_runs_once_for_every_way_a_slot_gives_up_its_value() {
    let source = r#"
struct Noisy { id: i32 }
impl Drop for Noisy {
    func drop(&mut self) { println("drop {self.id}") }
}

func main() -> i32 {
    mut v: Vec<Noisy> = Vec::new()
    v.push(Noisy { id: 1 })
    v.push(Noisy { id: 2 })
    v.push(Noisy { id: 3 })
    v.push(Noisy { id: 4 })
    v[1] = Noisy { id: 20 }
    println("overwritten")
    val taken = v.remove(0)
    println("removed {taken.id}")
    for n in v {
        println("visit {n.id}")
        if n.id == 20 { break }
    }
    println("loop left")
    mut m: HashMap<i32, Noisy> = HashMap::new()
    m.insert(7, Noisy { id: 70 })
    m.insert(7, Noisy { id: 71 })
    println("displaced")
    m.clear()
    println("cleared")
    0
}
"#;
    assert_eq!(
        stdout_of("drop_order.nr", source),
        "drop 2\noverwritten\nremoved 1\nvisit 20\ndrop 20\ndrop 3\ndrop 4\nloop left\n\
         drop 70\ndisplaced\ndrop 71\ncleared\ndrop 1\n"
    );
}

#[test]
fn nested_collections_and_builders_are_elements() {
    let source = r#"
func main() -> i32 {
    mut grid: Vec<Vec<i32>> = Vec::new()
    grid.push(Vec::new())
    grid.push(Vec::with_capacity(4))
    grid[0].push(4)
    grid[1].push(5)
    grid[1][0] += 1
    mut parts: Vec<StringBuilder> = Vec::new()
    parts.push(StringBuilder::new())
    parts[0].push_str("ab")
    parts[0].push_str("cd")
    val text = parts[0].to_string()
    mut index: HashMap<string, Vec<string>> = HashMap::new()
    index.insert("k", Vec::new())
    match index.get_mut("k") {
        Option::Some(list) => list.push("v"),
        Option::None => {}
    }
    val listed = match index.get("k") {
        Option::Some(list) => list.len() as i32,
        Option::None => 0
    }
    grid[0][0] + grid[1][0] + text.len() as i32 + listed
}
"#;
    assert_eq!(run("nested_elements.nr", source), 4 + 6 + 4 + 1);
}

#[test]
fn readers_lend_and_takers_hand_over() {
    let source = r#"
func main() -> i32 {
    mut m: BTreeMap<string, string> = BTreeMap::new()
    m.insert("a", "one" + "")
    val displaced = m.insert("a", "uno" + "")
    val old_len = match displaced {
        Option::Some(old) => old.len() as i32,
        Option::None => 0
    }
    val lent_len = match m.get("a") {
        Option::Some(s) => s.len() as i32,
        Option::None => 0
    }
    match m.get_mut("a") {
        Option::Some(s) => { *s = "eins" + "" }
        Option::None => {}
    }
    val gone = m.remove("a")
    val gone_len = match gone {
        Option::Some(s) => s.len() as i32,
        Option::None => 0
    }
    mut v: Vec<i32> = Vec::new()
    v.push(5)
    val lent = v.get(0) ?? 0
    val missing = v.get(9) ?? -1
    (old_len * 1000 + lent_len * 100 + gone_len * 10 + lent + missing + m.len() as i32) % 256
}
"#;
    assert_eq!(
        run("readers_takers.nr", source),
        (3000 + 300 + 40 + 5 - 1) % 256
    );
}

#[test]
fn slot_releases_do_not_accumulate() {
    let source = r#"
struct Entry { key: string, notes: Vec<string> }

func entry(i: i32) -> Entry {
    mut notes: Vec<string> = Vec::new()
    notes.push("note {i}")
    Entry { key: "k{i}", notes: notes }
}

func main() -> i32 {
    mut i = 0
    mut n: u64 = 0
    while i < 100000 {
        mut v: Vec<Entry> = Vec::new()
        v.push(entry(i))
        v.push(entry(i + 1))
        v[0] = entry(i + 2)
        val out = v.swap_remove(0)
        n = n + out.key.len() + v[0].notes.len()
        mut m: HashMap<string, Entry> = HashMap::new()
        m.insert("x", entry(i))
        m.insert("x", entry(i))
        val gone = m.remove("x")
        for e in v {
            n = n + e.notes.len()
        }
        i = i + 1
    }
    if n == 0 { return 91 }
    0
}
"#;
    assert_eq!(run("slot_churn.nr", source), 0);
}

#[test]
fn a_by_value_element_read_is_refused_with_the_ways_out() {
    let err = rejected(
        "slot_move.nr",
        r#"
struct P { name: string }
func main() -> i32 {
    mut v: Vec<P> = Vec::new()
    v.push(P { name: "a" })
    val p = v[0]
    0
}
"#,
    );
    assert!(
        err.contains("cannot move a `P` out of a `Vec` slot")
            && err.contains("&v[i]")
            && err.contains("v[i].clone()")
            && err.contains("v.swap_remove(i)"),
        "got: {err}"
    );
}

#[test]
fn an_inserted_value_cannot_be_used_again() {
    let err = rejected(
        "inserted_moved.nr",
        r#"
func main() -> i32 {
    mut m: HashMap<string, Vec<i32>> = HashMap::new()
    val list: Vec<i32> = Vec::new()
    m.insert("a", list)
    list.len() as i32
}
"#,
    );
    assert!(err.contains("use of moved value 'list'"), "got: {err}");
}

#[test]
fn a_lent_slot_blocks_a_mutation_of_its_collection() {
    let err = rejected(
        "lent_slot.nr",
        r#"
func main() -> i32 {
    mut v: Vec<string> = Vec::new()
    v.push("a")
    val first = &v[0]
    v.push("b")
    first.len() as i32
}
"#,
    );
    assert!(err.contains("cannot borrow 'v' as mutable"), "got: {err}");
}

#[test]
fn a_borrowed_head_does_not_hand_its_elements_out() {
    let err = rejected(
        "lent_iteration.nr",
        r#"
struct P { name: string }
func keep(p: P) -> i32 { 0 }
func main() -> i32 {
    val arr: [P; 1] = [P { name: "a" }]
    for p in &arr {
        keep(*p)
    }
    0
}
"#,
    );
    assert!(err.contains("cannot move out of 'p'"), "got: {err}");
}

#[test]
fn a_field_and_an_element_borrow_read_in_place() {
    let source = r#"
struct P { name: string, scores: [i32; 3] }
func longest(a: &string, b: &string) -> i32 {
    if a.len() > b.len() { a.len() as i32 } else { b.len() as i32 }
}
func main() -> i32 {
    mut p = P { name: "ada", scores: [1, 2, 3] }
    val other: string = "bobby"
    val n = longest(&p.name, &other)
    {
        val s = &mut p.scores[1]
        *s += 40
    }
    n * 100 + p.scores[1]
}
"#;
    assert_eq!(
        run("sub_place_borrows.nr", source) as u32 & 0xff,
        (500 + 42) & 0xff
    );
}

#[test]
fn regression_bug_033_a_field_or_an_element_is_borrowed_in_place() {
    let source = r#"
struct Layer { w: Tensor<i32, [2, 2]> }

impl Layer {
    func forward(&self, x: &Tensor<i32, [2, 2]>) -> Tensor<i32, [2, 2]> {
        &self.w @ x
    }
}

struct Pair { a: i32 }
func take(r: &i32) -> i32 { *r }

func main() -> i32 {
    val l = Layer { w: [[1, 2], [3, 4]] }
    val x: Tensor<i32, [2, 2]> = [[5, 6], [7, 8]]
    val p = Pair { a: 2 }
    val arr = [10, 20]
    return l.forward(&x)[0, 0] + take(&p.a) + take(&arr[0])
}
"#;
    assert_eq!(run("bug_033.nr", source), 19 + 2 + 10);
}

/// The fixture the slot-position store tests share: a row with a `string` at every kind
/// of position a store can name.
const SLOT_ROW: &str = r#"
struct Inner { name: string }
struct Row { cells: Vec<i32>, tag: (i32, string), name: string, pair: [string; 2], inner: Inner }
struct Table { rows: Vec<Row> }

func row(i: i32) -> Row {
    Row {
        cells: Vec::new(),
        tag: (i, "t" + "{i}"),
        name: "n" + "{i}",
        pair: ["a" + "{i}", "b"],
        inner: Inner { name: "i" + "{i}" }
    }
}
"#;

/// A literal stored into a position inside a slot is copied to the heap like one stored
/// into the slot itself, so the slot's release never frees read-only data. Each of these
/// stores aborted with `free(): invalid pointer` when the `Vec` was destroyed.
#[test]
fn regression_a_literal_stored_inside_a_slot_is_owned_by_the_slot() {
    let source = format!(
        "{SLOT_ROW}{}",
        r#"
func main() -> i32 {
    mut total = 0
    for i in 0..10 {
        mut rows: Vec<Row> = Vec::new()
        rows.push(row(i))
        rows[0].name = "literal"
        rows[0].tag.1 = "tagged"
        rows[0].tag = (2, "whole")
        rows[0].pair[1] = "fixed"
        rows[0].pair[i % 2] = "runtime"
        rows[0].inner.name = "nested"
        rows[0].inner = Inner { name: "inner" }
        val bound = "bound"
        rows[0].name = bound
        mut grid: Vec<Vec<Row>> = Vec::new()
        grid.push(Vec::new())
        grid[0].push(row(i))
        grid[0][0].name = "deep"
        mut table = Table { rows: Vec::new() }
        table.rows.push(row(i))
        table.rows[0].inner.name = "held"
        mut pairs: Vec<(i32, string)> = Vec::new()
        pairs.push((1, "p" + "{i}"))
        pairs[0].1 = "second"
        total += rows[0].name.len() as i32 + rows[0].tag.1.len() as i32
            + rows[0].inner.name.len() as i32 + grid[0][0].name.len() as i32
            + table.rows[0].inner.name.len() as i32 + pairs[0].1.len() as i32
    }
    total % 256
}
"#
    );
    // bound 5 + whole 5 + inner 5 + deep 4 + held 4 + second 6, ten times.
    let exit = CompileTest::new().run_leak_checked("slot_position_literal.nr", &source);
    assert_eq!(exit, 290 % 256);
}

/// A store into a position inside a slot releases what it displaces, a `string` and a
/// typed owner alike, and a user `Drop` runs once for the value that left.
#[test]
fn regression_a_store_inside_a_slot_releases_what_it_displaces() {
    let source = format!(
        "{SLOT_ROW}{}",
        r#"
struct Noisy { id: i32 }
impl Drop for Noisy {
    func drop(&mut self) { println("drop {self.id}") }
}
struct Cage { pet: Noisy, label: string }

func main() -> i32 {
    mut total = 0
    for i in 0..10 {
        mut rows: Vec<Row> = Vec::new()
        rows.push(row(i))
        rows[0].name = "own" + "{i}"
        rows[0].name += "!"
        rows[0].tag.1 = "tag" + "{i}"
        rows[0].pair[0] = "left" + "{i}"
        rows[0].inner.name = "in" + "{i}"
        mut cells: Vec<i32> = Vec::new()
        cells.push(i)
        rows[0].cells = cells
        mut more: Vec<i32> = Vec::new()
        more.push(i)
        more.push(i)
        rows[0].cells = more
        total += rows[0].name.len() as i32 + rows[0].cells.len() as i32
    }
    mut cages: Vec<Cage> = Vec::new()
    cages.push(Cage { pet: Noisy { id: 1 }, label: "c" + "1" })
    cages[0].pet = Noisy { id: 2 }
    cages[0].label = "c" + "2"
    println("stored")
    total
}
"#
    );
    let test = CompileTest::new();
    // "own0!" is 5 bytes and the second `cells` holds 2, ten times.
    assert_eq!(
        test.run_leak_checked("slot_position_displaced.nr", &source),
        70
    );
    assert_eq!(
        stdout_of("slot_position_drop_order.nr", &source),
        "drop 1\nstored\ndrop 2\n"
    );
}
