// End-to-end tests for the three `for` receiver modes: `for x in &xs` binds a
// `&T` to each element, `for x in &mut xs` a `&mut T` that writes the element in
// place, and `for x in xs` consumes the head. Arrays, `Vec`s and slices follow the same
// rule for every element type, and `&mut it` drives a protocol `Iterator` in place.
use crate::compile_harness::{CompileTest, rejected, run, stdout_of};

#[test]
fn a_mutable_borrow_writes_every_element_in_place() {
    let source = r#"
func double(xs: &mut [i32]) {
    for x in xs { *x *= 2 }
}
func main() -> i32 {
    mut arr = [1, 2, 3]
    for x in &mut arr { *x += 10 }
    double(&mut arr)
    mut v: Vec<i32> = Vec::new()
    v.push(1)
    v.push(2)
    for x in &mut v { *x = *x * 100 }
    arr[0] + arr[2] + v[0] / 100 + v[1] / 100
}
"#;
    // arr = [22, 24, 26], v = [100, 200]
    assert_eq!(run("mut_borrow_writes.nr", source), 22 + 26 + 1 + 2);
}

/// A field write and a `&mut self` method call through the binding both reach the slot.
#[test]
fn a_mutable_borrow_reaches_a_struct_elements_fields_and_methods() {
    let source = r#"
struct Account { owner: string, balance: i32 }
impl Account {
    func deposit(&mut self, amount: i32) { self.balance += amount }
}
func main() -> i32 {
    mut book: Vec<Account> = Vec::new()
    book.push(Account { owner: "ada", balance: 5 })
    book.push(Account { owner: "bo", balance: 7 })
    for a in &mut book {
        a.balance += 1
        a.deposit(a.owner.len() as i32)
    }
    for a in &book { println("{a.owner} {a.balance}") }
    0
}
"#;
    assert_eq!(stdout_of("mut_borrow_fields.nr", source), "ada 9\nbo 10\n");
}

/// `&string` is the string's own fat pointer rather than its address, so a borrowed
/// loop over strings binds each element's bits and reads them in place.
#[test]
fn a_shared_borrow_reads_strings_in_place() {
    let source = r#"
func longest(words: &Vec<string>) -> u64 {
    mut best: u64 = 0
    for w in words {
        if w.len() > best { best = w.len() }
    }
    best
}
func main() -> i32 {
    mut words: Vec<string> = Vec::new()
    words.push("alpha")
    words.push("be")
    val arr = ["xyz", "q"]
    mut short: u64 = 0
    for w in &arr { if w.len() < 2 { short += 1 } }
    (longest(&words) * 10 + short + words.len()) as i32
}
"#;
    assert_eq!(run("shared_strings.nr", source), 53);
}

/// The binding owns nothing, so a borrowed loop releases nothing: each element is
/// dropped once, by its owner, after the loop.
#[test]
fn a_borrowed_loop_leaves_ownership_with_the_collection() {
    let source = r#"
struct Noisy { id: i32 }
impl Drop for Noisy {
    func drop(&mut self) { println("drop {self.id}") }
}
func main() -> i32 {
    mut v: Vec<Noisy> = Vec::new()
    v.push(Noisy { id: 1 })
    v.push(Noisy { id: 2 })
    for n in &v { println("see {n.id}") }
    for n in &mut v { n.id += 10 }
    println("end")
    0
}
"#;
    assert_eq!(
        stdout_of("borrowed_drops.nr", source),
        "see 1\nsee 2\nend\ndrop 11\ndrop 12\n"
    );
}

/// The position and adapter forms compose with a borrowed head, and an adapter's
/// function is handed the same `&T` the plain loop binds.
#[test]
fn enumerate_and_adapters_see_the_borrow() {
    let source = r#"
func main() -> i32 {
    val xs = [3, 4, 5]
    mut weighted = 0
    for (i, x) in (&xs).enumerate() { weighted += (i as i32) * x }
    mut big = 0
    for x in (&xs).map(|x: &i32| -> i32 { *x * 2 }).filter(|y: i32| -> bool { y > 6 }) {
        big += x
    }
    weighted * 10 + big
}
"#;
    // weighted = 0*3 + 1*4 + 2*5 = 14; doubled 6, 8, 10 keeps 8 + 10
    assert_eq!(run("borrowed_adapters.nr", source), 158);
}

/// `next` takes `&mut self`, so `&mut it` drives the iterator in place: a `break`
/// leaves it where it stopped, and the rest of the stream is still there.
#[test]
fn a_mutable_borrow_of_an_iterator_resumes_where_it_stopped() {
    let source = r#"
struct Count { n: i32 }
impl Iterator for Count {
    type Item = i32
    func next(&mut self) -> Option<i32> {
        if self.n >= 6 { return None }
        self.n += 1
        Some(self.n)
    }
}
func main() -> i32 {
    mut c = Count { n: 0 }
    mut head = 0
    for k in &mut c {
        head += k
        if k == 3 { break }
    }
    mut rest = 0
    for k in c { rest += k }
    head * 10 + rest
}
"#;
    // 1 + 2 + 3 before the break, 4 + 5 + 6 after it
    assert_eq!(run("iterator_in_place.nr", source), 75);
}

#[test]
fn a_borrowed_binding_is_a_reference_not_an_element() {
    let err = rejected(
        "borrowed_binding_type.nr",
        r#"
func main() -> i32 {
    val xs = [1, 2]
    for x in &xs {
        val y: i32 = x
    }
    0
}
"#,
    );
    assert!(err.contains("expected i32, found &i32"), "got: {err}");
}

#[test]
fn a_shared_borrow_cannot_write_through_its_binding() {
    let err = rejected(
        "shared_write.nr",
        r#"
func main() -> i32 {
    mut xs = [1, 2]
    for x in &xs { *x = 0 }
    0
}
"#,
    );
    assert!(
        err.contains("cannot assign through an immutable reference"),
        "got: {err}"
    );
}

/// A pushed element could move the buffer the loop is walking, so the head stays
/// borrowed for the whole loop, not for the statement that wrote it.
#[test]
fn the_head_is_borrowed_for_the_whole_loop() {
    let shared = rejected(
        "push_while_reading.nr",
        r#"
func main() -> i32 {
    mut v: Vec<i32> = Vec::new()
    v.push(1)
    for x in &v { v.push(*x) }
    0
}
"#,
    );
    assert!(
        shared.contains("cannot borrow 'v' as mutable"),
        "got: {shared}"
    );
    let exclusive = rejected(
        "read_while_writing.nr",
        r#"
func main() -> i32 {
    mut v: Vec<i32> = Vec::new()
    v.push(1)
    for x in &mut v { *x = v.len() as i32 }
    0
}
"#,
    );
    assert!(
        exclusive.contains("while it is mutably borrowed"),
        "got: {exclusive}"
    );
}

#[test]
fn a_mutable_borrow_needs_a_mut_binding() {
    let err = rejected(
        "immutable_head.nr",
        r#"
func main() -> i32 {
    val xs = [1, 2]
    for x in &mut xs { *x = 0 }
    0
}
"#,
    );
    assert!(err.contains("requires a `mut` binding"), "got: {err}");
}

/// `for x in v` consumes `v` whatever its element type.
#[test]
fn a_by_value_head_consumes_a_vec_of_copy_elements() {
    let err = rejected(
        "consumed_vec.nr",
        r#"
func main() -> i32 {
    mut v: Vec<i32> = Vec::new()
    v.push(1)
    mut total = 0
    for x in v { total += x }
    total + v.len() as i32
}
"#,
    );
    assert!(err.contains("use of moved value 'v'"), "got: {err}");
}

#[test]
fn a_by_value_head_copies_an_array_of_copy_elements() {
    let source = r#"
func main() -> i32 {
    val xs = [1, 2, 3]
    mut total = 0
    for x in xs { total += x }
    total + xs[2]
}
"#;
    assert_eq!(run("copied_array.nr", source), 9);
}

#[test]
fn a_shared_borrow_of_an_iterator_is_rejected() {
    let err = rejected(
        "shared_iterator.nr",
        r#"
struct Count { n: i32 }
impl Iterator for Count {
    type Item = i32
    func next(&mut self) -> Option<i32> { None }
}
func main() -> i32 {
    val c = Count { n: 0 }
    for k in &c { }
    0
}
"#,
    );
    assert!(
        err.contains("write `&mut` to drive an `Iterator` in place"),
        "got: {err}"
    );
}

/// A by-value `for` over an array hands each element's ownership to the binding on its
/// pass, and the array keeps what the loop never reached.
#[test]
fn regression_bug_124_a_by_value_array_loop_releases_every_element() {
    let source = r#"
struct N { a: string, k: i32 }
struct V { v: Vec<i32> }

func q() -> string { "a" + "b" }
func held(k: i32) -> V {
    mut w: Vec<i32> = Vec::new()
    w.push(k)
    V { v: w }
}

func all() -> i32 {
    val parts = [q(), "x" + "y", "lit"]
    mut total = 0
    for part in parts { total += part.len() as i32 }
    total
}

func broken() -> i32 {
    val parts = [q(), "x" + "y", "lit"]
    mut total = 0
    for part in parts {
        total += part.len() as i32
        break
    }
    total
}

func returned() -> i32 {
    val parts = [q(), "x" + "y"]
    for part in parts {
        return part.len() as i32 + 1
    }
    0
}

func moved() -> i32 {
    val parts = [q(), "x" + "yz"]
    mut v: Vec<string> = Vec::new()
    for part in parts { v.push(part) }
    v[1].len() as i32 + v.len() as i32
}

func structs() -> i32 {
    val xs = [N { a: "p" + "q", k: 1 }, N { a: "r" + "st", k: 2 }]
    mut t = 0
    for x in xs { t += x.a.len() as i32 + x.k }
    t
}

func owners() -> i32 {
    val xs = [held(1), held(2), held(3)]
    mut t = 0
    for x in xs {
        t += x.v[0]
        if t == 3 { break }
    }
    t
}

func main() -> i32 {
    mut t = 0
    for i in 0..4 { t += all() + broken() + returned() + moved() + structs() + owners() }
    t
}
"#;
    let test = CompileTest::new();
    assert_eq!(test.run_leak_checked("array_loop.nr", source), 112);
}
