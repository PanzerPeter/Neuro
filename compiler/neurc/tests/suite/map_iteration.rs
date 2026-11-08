//! `HashMap` / `BTreeMap` as `for` heads (§3.7, §3.18): the entry by value, `(&K, &V)`
//! through `&m`, `(&K, &mut V)` through `&mut m`, the `m.keys()` / `m.values()` views, a
//! pair head over any two-element tuple item, and a key type parameter with no `Copy`
//! bound.

use super::compile_harness::{CompileTest, assert_check_error, run, stdout_of};

/// The ordered map yields ascending keys through every head, and a borrowed head leaves
/// the map usable.
#[test]
fn an_ordered_map_is_walked_in_key_order_through_a_borrow() {
    let out = stdout_of(
        "map_iter_order.nr",
        r#"
func main() -> i32 {
    mut ages: BTreeMap<string, i32> = BTreeMap::new()
    ages.insert("cleo", 31)
    ages.insert("ada", 36)
    ages.insert("bo", 4)
    for (name, age) in &ages {
        println("{name} {age}")
    }
    for k in ages.keys() {
        println("key {k}")
    }
    for v in ages.values() {
        println("value {v}")
    }
    ages.len() as i32
}
"#,
    );
    assert_eq!(
        out,
        "ada 36\nbo 4\ncleo 31\nkey ada\nkey bo\nkey cleo\nvalue 36\nvalue 4\nvalue 31\n"
    );
}

/// `&mut m` lends each value mutably, so the write lands in the map.
#[test]
fn a_mutable_head_writes_each_value_in_place() {
    let exit = run(
        "map_iter_mut.nr",
        r#"
func main() -> i32 {
    mut h: HashMap<i32, i32> = HashMap::new()
    h.insert(1, 10)
    h.insert(2, 20)
    h.insert(3, 30)
    for (k, v) in &mut h {
        *v += *k
    }
    mut b: BTreeMap<i32, string> = BTreeMap::new()
    b.insert(1, "a")
    for e in &mut b {
        e.1.len()
    }
    match h.get(3) {
        Option::Some(v) => *v,
        Option::None => 0,
    }
}
"#,
    );
    assert_eq!(exit, 33);
}

/// A hashed table leaves gaps between its live slots, and removal leaves tombstones; the
/// walk visits each live entry once, and the position of `.enumerate()` counts entries,
/// not slots.
#[test]
fn a_hashed_walk_visits_each_live_entry_once() {
    let exit = run(
        "map_iter_hash.nr",
        r#"
func main() -> i32 {
    mut h: HashMap<i32, i32> = HashMap::new()
    mut i = 0
    while i < 20 {
        h.insert(i, i * 2)
        i += 1
    }
    h.remove(4)
    h.remove(7)
    mut total = 0
    mut seen = 0
    for (pos, k) in h.keys().enumerate() {
        total += *k
        seen = pos as i32 + 1
    }
    for v in h.values() {
        total += *v
    }
    total + seen
}
"#,
    );
    // keys 0..20 minus 4 and 7 sum to 179, values to 358, and 18 entries were seen.
    assert_eq!(exit, (179 + 358 + 18) % 256);
}

/// A by-value head moves each entry out; `break` and `return` release exactly the
/// entries no pass took, and an empty map runs the body zero times.
#[test]
fn a_by_value_head_consumes_and_releases_what_it_leaves() {
    let source = r#"
struct Tag { label: string, n: i32 }

func build(i: i32) -> HashMap<string, Tag> {
    mut m: HashMap<string, Tag> = HashMap::new()
    m.insert("a{i}", Tag { label: "x{i}", n: 1 })
    m.insert("b{i}", Tag { label: "y{i}", n: 2 })
    m.insert("c{i}", Tag { label: "z{i}", n: 3 })
    m.remove("b{i}")
    m
}

func ordered(i: i32) -> BTreeMap<string, string> {
    mut m: BTreeMap<string, string> = BTreeMap::new()
    m.insert("k{i}", "v{i}")
    m.insert("a{i}", "b{i}")
    m
}

func first_label(m: HashMap<string, Tag>) -> i32 {
    for (k, t) in m {
        return t.label.len() as i32 + k.len() as i32
    }
    0
}

func main() -> i32 {
    mut sum = 0
    mut i = 0
    while i < 10 {
        for (k, t) in build(i) {
            sum += t.n
            if t.n == 3 { break }
        }
        for e in ordered(i) {
            sum += e.0.len() as i32
            break
        }
        sum += first_label(build(i))
        val empty: BTreeMap<string, string> = BTreeMap::new()
        for e in empty { sum += 100 }
        i += 1
    }
    sum % 256
}
"#;
    let exit = CompileTest::new().run_leak_checked("map_iter_consume.nr", source);
    assert!((0..256).contains(&exit), "exit {exit}");
}

/// The views lend into the map: a `string` key or value is read in place, never copied.
#[test]
fn the_views_bind_borrows_into_the_map() {
    let source = r#"
func main() -> i32 {
    mut total = 0
    mut i = 0
    while i < 10 {
        mut m: BTreeMap<string, string> = BTreeMap::new()
        m.insert("key{i}", "value{i}")
        m.insert("k{i}", "v{i}")
        for k in m.keys() { total += k.len() as i32 }
        for v in m.values() { total += v.len() as i32 }
        for (k, v) in &m { total += k.len() as i32 - v.len() as i32 }
        i += 1
    }
    total
}
"#;
    // Per round: keys 4 + 2, values 6 + 2, then the pairs give back -2 and 0.
    assert_eq!(
        CompileTest::new().run_leak_checked("map_iter_views.nr", source),
        10 * (4 + 2 + 6 + 2 - 2)
    );
}

/// The head adapters fold over the views as over any other head.
#[test]
fn the_views_take_head_adapters() {
    let exit = run(
        "map_iter_adapters.nr",
        r#"
func main() -> i32 {
    mut m: HashMap<i32, i32> = HashMap::new()
    m.insert(1, 1)
    m.insert(2, 5)
    m.insert(3, 9)
    mut total = 0
    for v in m.values().map(|v| *v * 10).filter(|v| v > 20) {
        total += v
    }
    for (i, k) in m.keys().filter(|k| *k > 1).enumerate() {
        total += i as i32
    }
    total
}
"#,
    );
    assert_eq!(exit, 140 + 1);
}

/// A pair head binds any two-element tuple item, a stored `Enumerate` included.
#[test]
fn a_pair_head_destructures_any_tuple_item() {
    let exit = run(
        "map_iter_pairs.nr",
        r#"
func main() -> i32 {
    val e = (10..13).enumerate()
    mut total = 0
    for (i, x) in e {
        total += i as i32 * x
    }
    val pairs = [(1, 2), (3, 4)]
    for (a, _) in pairs {
        total += a
    }
    total
}
"#,
    );
    assert_eq!(exit, 11 + 24 + 4);
}

/// A type parameter key moves into its slot like a value, so it needs no `Copy` bound,
/// and a generic body walks the map it built.
#[test]
fn a_generic_key_needs_no_copy_bound() {
    let source = r#"
@derive(PartialEq, Hashable)
struct Name { text: string }

impl Comparable for Name {
    func lt(&self, rhs: &Name) -> bool { self.text.len() < rhs.text.len() }
}

func tally<K: Hashable + PartialEq>(keys: Vec<K>) -> HashMap<K, i32> {
    mut m: HashMap<K, i32> = HashMap::new()
    for k in keys {
        if !m.contains_key(k) { m.insert(k, 1) }
    }
    m
}

func ordered<K: Comparable + PartialEq>(keys: Vec<K>) -> i32 {
    mut m: BTreeMap<K, i32> = BTreeMap::new()
    for k in keys { m.insert(k, 1) }
    mut n = 0
    for (_, v) in &m { n += *v }
    n
}

func main() -> i32 {
    mut words: Vec<string> = Vec::new()
    words.push("a")
    words.push("bb")
    words.push("a")
    val counts = tally(words)
    mut names: Vec<Name> = Vec::new()
    names.push(Name { text: "x" })
    names.push(Name { text: "yy" })
    names.push(Name { text: "x" })
    val by_name = tally(names)
    mut total = 0
    for (w, n) in &counts { total += *n }
    for k in by_name.keys() { total += k.text.len() as i32 }
    mut more: Vec<Name> = Vec::new()
    more.push(Name { text: "q" })
    more.push(Name { text: "rr" })
    total + ordered(more)
}
"#;
    assert_eq!(
        CompileTest::new().run_leak_checked("map_iter_generic_key.nr", source),
        2 + 3 + 2
    );
}

#[test]
fn keys_outside_a_head_are_refused() {
    assert_check_error(
        "map_keys_value.nr",
        r#"
func main() -> i32 {
    val m: HashMap<i32, i32> = HashMap::new()
    val ks = m.keys()
    0
}
"#,
        "`.keys()` walks a map's entries by reference in a `for` head",
    );
}

#[test]
fn values_outside_a_head_are_refused() {
    assert_check_error(
        "map_values_value.nr",
        r#"
func main() -> i32 {
    val m: BTreeMap<i32, i32> = BTreeMap::new()
    m.values().len() as i32
}
"#,
        "`.values()` walks a map's entries by reference in a `for` head",
    );
}

#[test]
fn a_pair_head_over_a_single_item_is_refused() {
    assert_check_error(
        "map_pair_single.nr",
        r#"
func main() -> i32 {
    val m: HashMap<i32, i32> = HashMap::new()
    for (a, b) in m.values() { }
    0
}
"#,
        "needs a two-element tuple item, and this head yields &i32",
    );
}

#[test]
fn inserting_while_the_map_is_lent_is_refused() {
    assert_check_error(
        "map_insert_under_borrow.nr",
        r#"
func main() -> i32 {
    mut m: HashMap<i32, i32> = HashMap::new()
    for k in m.keys() { m.insert(*k + 1, 0) }
    0
}
"#,
        "cannot borrow 'm' as mutable",
    );
}

#[test]
fn reading_the_map_during_a_mutable_walk_is_refused() {
    assert_check_error(
        "map_read_under_mut.nr",
        r#"
func main() -> i32 {
    mut m: HashMap<i32, i32> = HashMap::new()
    for (k, v) in &mut m { val n = m.len() }
    0
}
"#,
        "cannot use 'm' while it is mutably borrowed",
    );
}

#[test]
fn a_by_value_head_consumes_the_map() {
    assert_check_error(
        "map_used_after_walk.nr",
        r#"
func main() -> i32 {
    val m: HashMap<string, i32> = HashMap::new()
    for e in m { }
    m.len() as i32
}
"#,
        "use of moved value 'm'",
    );
}

/// A key is never lent mutably, and a view lends shared borrows only.
#[test]
fn keys_and_viewed_values_cannot_be_written() {
    assert_check_error(
        "map_write_key.nr",
        r#"
func main() -> i32 {
    mut m: HashMap<i32, i32> = HashMap::new()
    for (k, v) in &mut m { *k = 3 }
    0
}
"#,
        "cannot assign through an immutable reference",
    );
    assert_check_error(
        "map_write_view.nr",
        r#"
func main() -> i32 {
    mut m: HashMap<i32, i32> = HashMap::new()
    for v in m.values() { *v = 3 }
    0
}
"#,
        "cannot assign through an immutable reference",
    );
}

/// A borrow a view lends cannot outlive the map it lends from.
#[test]
fn a_viewed_borrow_of_a_local_map_cannot_be_returned() {
    assert_check_error(
        "map_view_escape.nr",
        r#"
func first() -> &i32 {
    val m: HashMap<i32, i32> = HashMap::new()
    for v in m.values() { return v }
    panic("empty")
}

func main() -> i32 { 0 }
"#,
        "cannot return a reference to 'm'",
    );
}
