// End-to-end tests for the borrowing iterator values: `.iter()` on an array, a slice or a
// `Vec` as the prelude's `Iter<T>`, and `m.keys()` / `m.values()` outside a `for` head as
// `HashKeys` / `HashValues` / `BTreeKeys` / `BTreeValues`. Each binds, passes, returns and
// takes the adapters; the container stays frozen while one lives; and one over a local
// cannot leave its function.
use crate::compile_harness::{CompileTest, assert_check_error, assert_exit, stdout_of};

/// A generic consumer over borrowed `i32` items, the shape most tests feed.
const TOTAL: &str = r#"
func total<I: Iterator<Item = &i32>>(it: I) -> i32 {
    mut t = 0
    for x in it {
        t += x
    }
    t
}

func numbers(n: i32) -> Vec<i32> {
    mut v: Vec<i32> = Vec::new()
    for i in 1..=n {
        v.push(i)
    }
    v
}
"#;

fn with_total(body: &str) -> String {
    format!("{TOTAL}{body}")
}

/// Every receiver shape `.iter()` takes: an owned `Vec`, an array, a slice view, a `&Vec`
/// and a `&mut [T]` parameter. Each yields borrows and leaves its container usable.
#[test]
fn iter_walks_every_contiguous_container_by_borrow() {
    let source = with_total(
        r#"
func through_borrow(v: &Vec<i32>) -> i32 {
    total(v.iter())
}

func through_mut_slice(xs: &mut [i32]) -> i32 {
    total(xs.iter())
}

func main() -> i32 {
    mut v = numbers(4)
    val arr = [10, 20, 30]
    val view = arr.slice(1..3)
    mut s = total(v.iter())
    s += total(arr.iter())
    s += total(view.iter())
    s += through_borrow(&v)
    s += through_mut_slice(&mut v)
    v.push(5)
    s + v.len() as i32 + arr[0]
}
"#,
    );
    assert_exit(
        "iter_receivers.nr",
        &source,
        10 + 60 + 50 + 10 + 10 + 5 + 10,
    );
}

/// An iterator over a parameter's elements may be returned, adapted first or not, and the
/// caller's container stays usable once it is spent.
#[test]
fn an_iterator_over_a_parameter_is_returned_through_adapters() {
    let source = with_total(
        r#"
func evens(v: &Vec<i32>) -> Filter<Iter<i32>, &i32> {
    v.iter().filter(|x| **x % 2 == 0)
}

func scaled(xs: &[i32], k: i32) -> Map<Iter<i32>, &i32, i32> {
    xs.iter().map(|x| x * k)
}

func main() -> i32 {
    val v = numbers(4)
    mut s = total(evens(&v))
    for x in scaled(&v, 10) {
        s += x
    }
    for (i, x) in v.iter().enumerate() {
        s += i as i32 * x
    }
    s + v[3]
}
"#,
    );
    assert_exit("iter_returned.nr", &source, 6 + 100 + 20 + 4);
}

/// `next` called directly answers a borrow of each element in turn, and a `&mut` head
/// resumes where a `break` left the walk. The `println` between the step and the
/// comparison is a call that reuses the stack a borrow of a copy would point into.
#[test]
fn an_iterator_value_is_driven_by_next_and_resumed() {
    let source = with_total(
        r#"
func main() -> i32 {
    val v = numbers(5)
    mut it = v.iter()
    mut s = 0
    for x in &mut it {
        s += x
        println("{x}")
        if *x == 2 {
            break
        }
    }
    while val Option::Some(x) = it.next() {
        s += 10 * x
    }
    s
}
"#,
    );
    assert_exit("iter_next.nr", &source, 3 + 120);
}

/// A `BTreeMap` view walks in ascending key order; a `HashMap` view visits every live entry
/// and skips the tombstone a removal leaves.
#[test]
fn map_views_walk_the_live_entries_as_values() {
    let source = r#"
func names(m: &BTreeMap<string, i32>) -> BTreeKeys<string, i32> {
    m.keys()
}

func main() -> i32 {
    mut b: BTreeMap<string, i32> = BTreeMap::new()
    b.insert("pear", 3)
    b.insert("apple", 1)
    b.insert("fig", 2)
    for k in names(&b) {
        println("{k}")
    }
    val ranks = b.values()
    for r in ranks {
        println("{r}")
    }
    mut h: HashMap<i32, i32> = HashMap::new()
    for i in 1..=5 {
        h.insert(i, i * 100)
    }
    h.remove(2)
    mut keys = 0
    for k in h.keys() {
        keys += k
    }
    val values = h.values().filter(|v| **v > 300)
    mut big = 0
    for v in values {
        big += v
    }
    println("{keys} {big}")
    0
}
"#;
    assert_eq!(
        stdout_of("map_views.nr", source),
        "apple\nfig\npear\n1\n2\n3\n13 900\n"
    );
}

/// A view through a `&mut` map parameter is a shared borrow, and a generic consumer takes
/// one over either map.
#[test]
fn a_map_view_passes_to_a_generic_consumer_through_any_borrow() {
    let source = with_total(
        r#"
func value_sum(m: &mut HashMap<string, i32>) -> i32 {
    total(m.values())
}

func main() -> i32 {
    mut h: HashMap<string, i32> = HashMap::new()
    h.insert("a", 4)
    h.insert("b", 5)
    mut b: BTreeMap<i32, i32> = BTreeMap::new()
    b.insert(7, 0)
    b.insert(3, 0)
    value_sum(&mut h) + total(b.keys())
}
"#,
    );
    assert_exit("map_view_generic.nr", &source, 9 + 10);
}

/// `string` elements, keys and values come out as borrows: nothing is copied, nothing is
/// released early, and nothing leaks.
#[test]
fn string_items_are_borrowed_without_leaking() {
    let source = r#"
func lengths<I: Iterator<Item = &string>>(it: I) -> u64 {
    mut n: u64 = 0
    for s in it {
        n += s.len()
    }
    n
}

func main() -> i32 {
    mut words: Vec<string> = Vec::new()
    words.push("alpha" + "")
    words.push("be" + "ta")
    mut m: BTreeMap<string, string> = BTreeMap::new()
    m.insert("k" + "ey", "va" + "lue")
    mut h: HashMap<string, string> = HashMap::new()
    h.insert("x" + "y", "z" + "")
    mut n = lengths(words.iter()) + lengths(m.keys()) + lengths(m.values())
    n += lengths(h.keys()) + lengths(h.values())
    val upper = words.iter().map(|w| w.len() * 2)
    for u in upper {
        n += u
    }
    n as i32 + words.len() as i32
}
"#;
    assert_eq!(
        CompileTest::new().run_leak_checked("iter_strings.nr", source),
        9 + 3 + 5 + 2 + 1 + 18 + 2
    );
}

/// The doubled even elements of `1..=6`, summed: 4 + 8 + 12 = 24, written as a tail
/// expression, through a binding, in a `match` arm, through a returned chain, and as the
/// borrowed head over the `&Vec` itself. All five must agree. The predicate reads through
/// its borrow, so one spelling serves an adapter value and a head form alike.
#[test]
fn equivalent_forms_agree() {
    let source = with_total(
        r#"
func chain(v: &Vec<i32>) -> Map<Filter<Iter<i32>, &i32>, &i32, i32> {
    v.iter().filter(|x| x % 2 == 0).map(|x| x * 2)
}

func tail(v: &Vec<i32>) -> i32 {
    mut s = 0
    for x in v.iter().filter(|x| x % 2 == 0).map(|x| x * 2) {
        s += x
    }
    s
}

func bound(v: &Vec<i32>) -> i32 {
    val doubled = v.iter().filter(|x| x % 2 == 0).map(|x| x * 2)
    mut s = 0
    for x in doubled {
        s += x
    }
    s
}

func in_arm(v: &Vec<i32>, on: bool) -> i32 {
    match on {
        true => {
            mut s = 0
            for x in chain(v) {
                s += x
            }
            s
        }
        false => 0
    }
}

func head_form(v: &Vec<i32>) -> i32 {
    mut s = 0
    for x in v {
        if *x % 2 == 0 {
            s += *x * 2
        }
    }
    s
}

func main() -> i32 {
    val v = numbers(6)
    val evens = total(v.iter().filter(|x| x % 2 == 0))
    val forms = [tail(&v), bound(&v), in_arm(&v, true), head_form(&v), evens * 2]
    for f in forms {
        if f != 24 {
            return f
        }
    }
    24
}
"#,
    );
    assert_exit("iter_equivalent_forms.nr", &source, 24);
}

/// The map views agree with the head forms they stand beside.
#[test]
fn map_view_values_agree_with_the_head_forms() {
    let source = r#"
func main() -> i32 {
    mut m: BTreeMap<i32, i32> = BTreeMap::new()
    for i in 1..=4 {
        m.insert(i * 3, i)
    }
    mut by_value = 0
    val ks = m.keys()
    for k in ks {
        by_value += k
    }
    mut by_head = 0
    for k in m.keys() {
        by_head += k
    }
    mut by_pair = 0
    for (k, _) in &m {
        by_pair += k
    }
    if by_value != by_head || by_head != by_pair {
        return 1
    }
    by_value
}
"#;
    assert_exit("map_view_forms.nr", source, 30);
}

/// A program's own `Iter` is its own type, and `.iter()` still answers the library's.
#[test]
fn a_program_iter_coexists_with_the_library_one() {
    let source = with_total(
        r#"
struct Iter { n: i32 }

func main() -> i32 {
    val v = numbers(3)
    total(v.iter()) + Iter { n: 4 }.n
}
"#,
    );
    assert_exit("iter_own_type.nr", &source, 10);
}

#[test]
fn the_container_is_frozen_while_a_view_lives() {
    assert_check_error(
        "iter_push_under_view.nr",
        r#"
func main() -> i32 {
    mut v: Vec<i32> = Vec::new()
    val it = v.iter()
    v.push(2)
    for x in it { }
    0
}
"#,
        "cannot borrow 'v' as mutable: it is already borrowed",
    );
    assert_check_error(
        "keys_insert_under_view.nr",
        r#"
func main() -> i32 {
    mut m: HashMap<i32, i32> = HashMap::new()
    val ks = m.keys()
    m.insert(1, 2)
    for k in ks { }
    0
}
"#,
        "cannot borrow 'm' as mutable: it is already borrowed",
    );
    assert_check_error(
        "iter_move_under_view.nr",
        r#"
func main() -> i32 {
    val v: Vec<i32> = Vec::new()
    val it = v.iter()
    val w = v
    for x in it { }
    0
}
"#,
        "cannot move out of 'v' while it is borrowed",
    );
}

/// An iterator over an owned local, `.chars()` included, cannot be returned: its
/// container dies with the frame.
#[test]
fn a_view_of_a_local_cannot_leave_its_function() {
    let message = "cannot return a reference to 'v': it is local to this function";
    assert_check_error(
        "iter_return_local.nr",
        "func f(v: Vec<i32>) -> Iter<i32> { v.iter() }\nfunc main() -> i32 { 0 }\n",
        message,
    );
    assert_check_error(
        "keys_return_local.nr",
        "func f(v: BTreeMap<i32, i32>) -> BTreeKeys<i32, i32> { v.keys() }\nfunc main() -> i32 { 0 }\n",
        message,
    );
    assert_check_error(
        "iter_return_local_binding.nr",
        r#"
func f() -> Iter<i32> {
    val v = [1, 2]
    v.iter()
}
func main() -> i32 { 0 }
"#,
        message,
    );
}

/// A block cannot yield an iterator over a binding it declares.
#[test]
fn a_view_of_a_block_local_cannot_leave_the_block() {
    assert_check_error(
        "iter_block_local.nr",
        r#"
func main() -> i32 {
    val it = {
        val v = [1, 2]
        v.iter()
    }
    0
}
"#,
        "'v' does not live long enough",
    );
}

/// Regression: `.chars()` over a by-value `string` parameter was returned, and the caller
/// walked text its callee had already released.
#[test]
fn regression_chars_of_a_local_cannot_be_returned() {
    assert_check_error(
        "chars_return_local.nr",
        "func f(v: string) -> Chars { v.chars() }\nfunc main() -> i32 { 0 }\n",
        "cannot return a reference to 'v': it is local to this function",
    );
}

/// A temporary container would die under its iterator, so it must be bound first.
#[test]
fn a_temporary_container_is_refused() {
    assert_check_error(
        "iter_temporary.nr",
        r#"
func make() -> Vec<i32> {
    Vec::new()
}
func main() -> i32 {
    val it = make().iter()
    0
}
"#,
        "cannot borrow this expression: `&` requires a place",
    );
}

/// An item borrows the container, not the iterator, so two results of `next()` are held
/// at once over every library view and over a program's iterator whose result names its
/// `impl`'s lifetime.
#[test]
fn two_items_of_one_iterator_are_held_at_once() {
    let source = r#"
func get(o: Option<&i32>) -> i32 {
    match o {
        Option::Some(x) => *x,
        Option::None => 0
    }
}

struct Walk<'a> { items: &'a [i32], at: u64 }

impl<'a> Walk<'a> {
    func step(&mut self) -> Option<&'a i32> {
        if self.at >= self.items.len() {
            return Option::None
        }
        val at = self.at
        self.at = at + 1
        Option::Some(&self.items[at])
    }
}

func main() -> i32 {
    mut v: Vec<i32> = Vec::new()
    v.push(1)
    v.push(2)
    mut it = v.iter()
    val a = it.next()
    val b = it.next()
    mut m: BTreeMap<i32, i32> = BTreeMap::new()
    m.insert(1, 30)
    m.insert(2, 40)
    mut vals = m.values()
    val c = vals.next()
    val d = vals.next()
    val text = "ab\ncde"
    mut lines = text.lines()
    val e = lines.next() ?? ""
    val f = lines.next() ?? ""
    val xs = [5, 6]
    mut w = Walk { items: &xs, at: 0 }
    val g = w.step()
    val h = w.step()
    println("{get(a) + get(b)} {get(c) + get(d)} {e.len() + f.len()} {get(g) + get(h)}")
    0
}
"#;
    assert_eq!(stdout_of("two_items.nr", source), "3 70 5 11\n");
}

/// The container stays frozen while an item lives, a method whose result elides its
/// lifetime still borrows its receiver, and so does one returning a view of `self`.
#[test]
fn an_item_keeps_its_container_frozen() {
    let message = "cannot borrow 'v' as mutable: it is already borrowed";
    assert_check_error(
        "item_push_under_item.nr",
        r#"
func main() -> i32 {
    mut v: Vec<i32> = Vec::new()
    v.push(3)
    mut it = v.iter()
    val a = it.next()
    v.push(9)
    match a { Option::Some(x) => *x, Option::None => 0 }
}
"#,
        message,
    );
    assert_check_error(
        "elided_getter.nr",
        r#"
struct Cell { v: i32 }
impl Cell {
    func get(&self) -> &i32 { &self.v }
    func set(&mut self, x: i32) { self.v = x }
}
func main() -> i32 {
    mut v = Cell { v: 1 }
    val r = v.get()
    v.set(5)
    *r
}
"#,
        message,
    );
    assert_check_error(
        "view_of_self.nr",
        r#"
struct Bag { items: Vec<i32> }
impl Bag {
    func walk(&self) -> Iter<i32> { self.items.iter() }
}
func main() -> i32 {
    mut v = Bag { items: Vec::new() }
    val it = v.walk()
    v.items.push(1)
    for x in it { }
    0
}
"#,
        message,
    );
}

/// Regression: a builtin mutating method on a field (`b.v.push(x)`) skipped the borrow
/// check its binding's own call gets, and a reader on one (`b.v.get(0)`) lent nothing for
/// its statement, so a borrow of `b.v` was left dangling.
#[test]
fn regression_a_field_is_not_mutated_under_a_borrow() {
    assert_check_error(
        "field_push_under_get.nr",
        r#"
struct Bag { v: Vec<i32> }
func main() -> i32 {
    mut b = Bag { v: Vec::new() }
    b.v.push(1)
    match b.v.get(0) {
        Option::Some(x) => {
            b.v.push(2)
            *x
        }
        Option::None => 0
    }
}
"#,
        "cannot borrow 'b' as mutable: it is already borrowed",
    );
    assert_check_error(
        "field_push_under_view.nr",
        r#"
struct Bag { v: Vec<i32> }
func main() -> i32 {
    mut b = Bag { v: Vec::new() }
    val r = &b.v
    b.v.push(1)
    r.len() as i32
}
"#,
        "cannot borrow 'b' as mutable: it is already borrowed",
    );
}
