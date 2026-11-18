// A `string` records in the top bit of its length whether it owns its buffer, so every
// `string` position is an owner like any other: a struct field, an enum payload, a
// by-value argument, a return value and a store through `&mut` release their buffer by
// the ordinary drop rules, and a literal reaching any of them is skipped at run time.
// Every program here runs under the leak check, which fails on a leak or a double free.

use crate::compile_harness::{CompileTest, rejected, stdout_of};

fn leak_checked(name: &str, source: &str) -> i32 {
    CompileTest::new().run_leak_checked(name, source)
}

/// BUG-077: a store through a borrow releases the `string` it displaces.
#[test]
fn regression_bug_077_a_store_through_a_borrow_releases_what_it_displaces() {
    let exit = leak_checked(
        "bug_077.nr",
        r#"
struct Named { name: string }

func rename(n: &mut Named, i: i32) { n.name = "item {i}" }
func set(s: &mut string, i: i32) { *s = "s{i}" }
func put(t: &mut (string, i32), v: string) { t.0 = v }
func gput<T>(t: &mut (T, i32), v: T) { t.0 = v }

func main() -> i32 {
    mut n = Named { name: "a" + "b" }
    mut s = "x" + "y"
    mut b = ("x" + "y", 1)
    mut g = ("p" + "q", 2)
    mut i = 0
    while i < 100 {
        rename(&mut n, i)
        set(&mut s, i)
        put(&mut b, "abc")
        put(&mut b, "d" + "{i}")
        gput(&mut g, "lit")
        gput(&mut g, "e" + "f")
        i += 1
    }
    (n.name.len() + s.len() + b.0.len() + g.0.len()) as i32
}
"#,
    );
    // "item 99", "s99", "d99" and "ef".
    assert_eq!(exit, 15);
}

/// BUG-082: an enum payload owns its `string`, and a literal payload owns nothing.
#[test]
fn regression_bug_082_an_enum_payload_releases_its_string() {
    let exit = leak_checked(
        "bug_082.nr",
        r#"
enum Msg {
    Text(string),
    Num(i32)
}

func make() -> u64 {
    val m = Msg::Text("ab" + "cd")
    val lit = Msg::Text("static")
    7
}

func take(m: Msg) -> u64 {
    match m {
        Msg::Text(t) => t.len(),
        Msg::Num(n) => n as u64,
    }
}

func main() -> i32 {
    mut t: u64 = 0
    for i in 0..5 {
        t = t + make()
    }
    t = t + take(Msg::Text("x" + "yz")) + take(Msg::Text("lit")) + take(Msg::Num(4))
    t as i32
}
"#,
    );
    assert_eq!(exit, 45);
}

/// BUG-085: a struct passed by value releases the `string` buffers it holds, through a
/// plain parameter and a consuming `self` alike.
#[test]
fn regression_bug_085_a_struct_passed_by_value_releases_its_strings() {
    let exit = leak_checked(
        "bug_085.nr",
        r#"
struct C { label: string, n: i32 }

impl C {
    func into_n(self) -> i32 { self.n }
}

func take(c: C) -> i32 { c.n }

func main() -> i32 {
    mut t = 0
    for i in 0..10 {
        val a = C { label: "a" + "bc", n: 1 }
        t = t + a.into_n()
        val b = C { label: "a" + "bcd", n: 1 }
        t = t + take(b)
    }
    t
}
"#,
    );
    assert_eq!(exit, 20);
}

/// A function whose exits mix a literal, a forwarded parameter and an allocation hands
/// each one back with its own owner bit.
#[test]
fn exits_mixing_a_literal_a_parameter_and_an_allocation_are_each_released() {
    let exit = leak_checked(
        "mixed_exits.nr",
        r#"
func label(c: bool, s: string) -> string {
    if c { return "none" }
    if s.len() > 2 { s } else { s + "!" }
}

func main() -> i32 {
    mut total: u64 = 0
    for i in 0..10 {
        total += label(i % 3 == 0, "ab" + "{i}").len()
    }
    total as i32
}
"#,
    );
    assert_eq!(exit, 34);
}

/// A parameter the callee keeps (in a collection behind `&mut`, or by returning it) and
/// one it drops are both released exactly once.
#[test]
fn a_parameter_the_callee_keeps_or_drops_is_released_once() {
    let exit = leak_checked(
        "kept_parameter.nr",
        r#"
struct Bag { items: Vec<string> }

func keep(b: &mut Bag, s: string) { b.items.push(s) }
func echo(s: string) -> string { s }
func ignore(s: string) -> u64 { 1 }

func main() -> i32 {
    mut bag = Bag { items: Vec::new() }
    mut total: u64 = 0
    for i in 0..10 {
        keep(&mut bag, "k{i}")
        val e = echo("e" + "{i}")
        total += e.len() + ignore("i" + "{i}")
    }
    (total + bag.items.len()) as i32
}
"#,
    );
    assert_eq!(exit, 40);
}

/// A `string` crossing a call to a function value with no body in view (a parameter, a
/// closure) moves into it, and the callee releases or returns it.
#[test]
fn a_string_through_a_function_value_is_released() {
    let exit = leak_checked(
        "function_value.nr",
        r#"
func apply(f: (string) -> string, s: string) -> string { f(s) }
func shout(s: string) -> string { s + "!" }

func main() -> i32 {
    mut total: u64 = 0
    for i in 0..10 {
        total += apply(shout, "a" + "{i}").len()
        total += apply(|s: string| -> string { s + "?" }, "q" + "r").len()
    }
    total as i32
}
"#,
    );
    assert_eq!(exit, 60);
}

/// A `string` parameter captured by value, and one a closure hands back, each have one
/// owner at a time: the environment, then the caller.
#[test]
fn a_captured_string_parameter_is_released_by_its_environment() {
    let exit = leak_checked(
        "captured_parameter.nr",
        r#"
func later(s: string) -> () -> u64 { move || s.len() }

func main() -> i32 {
    mut total: u64 = 0
    for i in 0..20 {
        val f = later("ab" + "{i}")
        total += f()
        val s = "cd" + "{i}"
        val g = || s
        val t = g()
        total += t.len()
    }
    total as i32
}
"#,
    );
    // Ten one-digit rounds and ten two-digit rounds, two strings each.
    assert_eq!(exit, 140);
}

/// A view of a `string` temporary keeps it alive to the end of its statement, where it
/// is released.
#[test]
fn a_view_of_a_temporary_is_released_at_the_end_of_its_statement() {
    let source = r#"
func main() -> i32 {
    mut total: u64 = 0
    for i in 0..10 {
        val a = "ab"
        total += (a + "{i}").slice(0..2).len()
        for c in (a + "xyz").chars() {
            total += 1
        }
        println((a + "cd").char_slice(1..3))
    }
    total as i32
}
"#;
    assert_eq!(leak_checked("temporary_view.nr", source), 70);
    assert_eq!(
        stdout_of("temporary_view_out.nr", source),
        "bc\n".repeat(10)
    );
}

/// A view of a `string` temporary cannot outlive its statement; a view of a literal can,
/// since nothing releases a literal.
#[test]
fn a_view_of_a_temporary_may_not_be_bound() {
    let err = rejected(
        "bound_view.nr",
        r#"
func main() -> i32 {
    val a = "ab"
    val v = (a + "cd").slice(0..2)
    v.len() as i32
}
"#,
    );
    assert!(err.contains("may outlive its statement"), "{err}");

    let exit = leak_checked(
        "literal_view.nr",
        r#"
func main() -> i32 {
    val v = "abcd".slice(0..2)
    val it = "xyz".chars()
    v.len() as i32
}
"#,
    );
    assert_eq!(exit, 2);
}

/// A struct taken out of a collection owns its `string` fields, through `while val`, a
/// `match` on `pop` and a function returning what it removed.
#[test]
fn a_value_taken_out_of_a_slot_owns_its_strings() {
    let exit = leak_checked(
        "taken_out.nr",
        r#"
struct Row { name: string, id: i32 }

func first(v: &mut Vec<Row>) -> Row { v.remove(0) }

func main() -> i32 {
    mut total: u64 = 0
    for k in 0..5 {
        mut rows: Vec<Row> = Vec::new()
        rows.push(Row { name: "r" + "{k}", id: 1 })
        rows.push(Row { name: "lit", id: 2 })
        while val Some(row) = rows.pop() {
            total += row.name.len() + row.id as u64
        }
        mut more: Vec<Row> = Vec::new()
        more.push(Row { name: "x" + "y", id: 0 })
        more.push(Row { name: "z", id: 0 })
        val r = first(&mut more)
        total += r.name.len()
        match more.pop() {
            Some(p) => { total += p.name.len() }
            None => {}
        }
    }
    total as i32
}
"#,
    );
    assert_eq!(exit, 55);
}

/// A holder built by a call that fills a position on only some paths, by a branch
/// inside a literal position, or by a value `loop` owns what it holds.
#[test]
fn a_holder_built_by_a_call_a_branch_or_a_loop_owns_its_strings() {
    let exit = leak_checked(
        "built_holders.nr",
        r#"
struct P { name: string, n: i32 }
struct W { inner: P }

func wrap(c: bool, s: string) -> P {
    if c { P { name: s, n: 1 } } else { P { name: "lit", n: 2 } }
}

func main() -> i32 {
    mut total: u64 = 0
    for i in 0..10 {
        val p = wrap(i % 2 == 0, "w" + "{i}")
        total += p.name.len()
        val w = W { inner: if i % 3 == 0 { P { name: "a" + "b", n: 0 } } else { P { name: "c", n: 0 } } }
        total += w.inner.name.len()
        val q = loop {
            break P { name: "l" + "p", n: 0 }
        }
        total += q.name.len()
    }
    total as i32
}
"#,
    );
    assert_eq!(exit, 59);
}

/// A `match` on a field or an element by run-time index moves the payload out of that
/// place, so the arm's binding and the holder do not both release it.
#[test]
fn regression_a_match_on_a_field_place_moves_the_payload_once() {
    let exit = leak_checked(
        "field_scrutinee.nr",
        r#"
struct Holder { opt: Option<string>, list: Option<Vec<i32>>, name: string }

func make(i: i32) -> Holder {
    mut v: Vec<i32> = Vec::new()
    v.push(i)
    Holder { opt: Some("v{i}"), list: Some(v), name: "n" + "{i}" }
}

func main() -> i32 {
    mut total: u64 = 0
    for i in 0..3 {
        val h = make(i)
        match h.opt {
            Some(s) => { total += s.len() }
            None => {}
        }
        match h.list {
            Some(v) => { total += v.len() }
            None => {}
        }
        total += h.name.len()
        val arr = [Some("c" + "d"), None]
        match arr[(i % 2) as u64] {
            Some(s) => { total += s.len() }
            None => {}
        }
    }
    total as i32
}
"#,
    );
    assert_eq!(exit, 19);
}

/// The same owned `string` reaches its reader nine ways: a tail, a bound-then-returned
/// local, a nested branch, a generic, a closure, a call argument, a by-value struct, an
/// enum payload and a store through `&mut`. Every one reads four bytes.
#[test]
fn an_owned_string_agrees_across_equivalent_forms() {
    let exit = leak_checked(
        "equivalent_forms.nr",
        r#"
struct Holder { s: string }

func tail() -> string { "ab" + "cd" }
func bound() -> string {
    val s = "ab" + "cd"
    s
}
func nested(c: bool) -> string {
    match c {
        true => "ab" + "cd",
        false => if c { "x" } else { "a" + "bcd" },
    }
}
func id<T>(x: T) -> T { x }
func len_of(s: string) -> u64 { s.len() }
func held(h: Holder) -> u64 { h.s.len() }
func put(t: &mut string) { *t = "ab" + "cd" }

func main() -> i32 {
    for i in 0..10 {
        if tail().len() != 4 { return 1 }
        if bound().len() != 4 { return 2 }
        if nested(i % 2 == 0).len() != 4 { return 3 }
        if id(tail()).len() != 4 { return 4 }
        if (|s: string| -> string { s })(bound()).len() != 4 { return 5 }
        if len_of(tail()) != 4 { return 6 }
        if held(Holder { s: tail() }) != 4 { return 7 }
        match Some(bound()) {
            Some(s) => { if s.len() != 4 { return 8 } }
            None => { return 8 }
        }
        mut t = "x"
        put(&mut t)
        if t.len() != 4 { return 9 }
    }
    0
}
"#,
    );
    assert_eq!(exit, 0);
}

/// An arm that binds nothing releases a temporary scrutinee: the `Err` a `??` fallback
/// replaces, and the value an `if val` does not match.
#[test]
fn regression_an_arm_binding_nothing_releases_a_temporary_scrutinee() {
    let exit = leak_checked(
        "unbound_scrutinee.nr",
        r#"
func find(i: i32) -> Result<i32, string> {
    if i % 2 == 0 { Ok(i) } else { Err("missing " + "{i}") }
}

func name(i: i32) -> Option<string> {
    if i > 100 { None } else { Some("n" + "{i}") }
}

func main() -> i32 {
    mut total = 0
    for i in 0..10 {
        total += find(i) ?? continue
        total += find(i + 1) ?? 1
        if val None = name(i) {
            total += 100
        }
    }
    total
}
"#,
    );
    // Odd `i` continues at once; even `i` adds itself and the `1` fallback.
    assert_eq!(exit, 25);
}
