// A string literal is accepted where a `&string` is expected, as an unsuffixed numeric literal
// takes its type from context. The literal is a view of read-only data: nothing is borrowed,
// and nothing is released, so the programs that store or return one run under the leak check.

use crate::compile_harness::{CompileTest, rejected, stdout_of};

fn leak_checked(name: &str, source: &str) -> i32 {
    CompileTest::new().run_leak_checked(name, source)
}

#[test]
fn a_literal_is_a_view_in_every_position_that_expects_one() {
    let out = stdout_of(
        "literal_positions.nr",
        r#"
struct Named { name: &string }
struct Greeter { n: u64 }
impl Greeter {
    func greet(&self, who: &string) -> u64 { who.len() + self.n }
}
enum Tag { Label(&string), Blank }
func len_of(s: &string) -> u64 { s.len() }
func pick(c: bool) -> &string { if c { "yes" } else { "no" } }
func named(label: &string, n: u64) -> u64 { label.len() + n }

func main() -> i32 {
    val annotated: &string = "annotated"
    mut assigned: &string = "one"
    assigned = "three"
    val field = Named { name: "field" }
    val opt: Option<&string> = Some("opt")
    val arr: [&string; 2] = ["p", "qq"]
    val tup: (&string, i32) = ("tup", 4)
    val tag = Tag::Label("tag")
    val tagged = match tag { Tag::Label(s) => s, Tag::Blank => "blank" }
    val none: Option<&string> = None
    val fallback = none ?? "fallback"
    val block: &string = { "block" }
    val arm: &string = match 1 { 1 => "arm", _ => "other" }
    val fv: (&string) -> u64 = len_of
    val g = Greeter { n: 1 }
    val dedented = len_of("""
        ab
        """)
    val counts = len_of("abc") + g.greet("abcd") + named(label: "xy", n: 3) + fv("12345") + dedented
    val same = assigned == "three"
    val x = opt ?? "none"
    println("{annotated} {assigned} {field.name} {x} {arr[1]} {tup.0} {tagged}")
    println("{fallback} {block} {arm} {pick(true)} {pick(false)} {counts} {same}")
    0
}
"#,
    );
    // 3 + (4 + 1) + (2 + 3) + 5 + 2, the block literal dedented to "ab".
    assert_eq!(
        out,
        "annotated three field opt qq tup tag\nfallback block arm yes no 20 true\n"
    );
}

#[test]
fn a_literal_reaches_a_generic_callee_through_a_bound_or_concrete_parameter() {
    let out = stdout_of(
        "literal_generic.nr",
        r#"
func id<T>(x: &T) -> &T { x }
func tagged<T>(x: T, s: &string) -> u64 { s.len() }
struct Pair<T> { v: T }
impl<T> Pair<T> {
    func label(&self, s: &string) -> u64 { s.len() + 100 }
}
trait Shout { func shout(&self, s: &string) -> u64 }
struct Loud { k: u64 }
impl Shout for Loud { func shout(&self, s: &string) -> u64 { s.len() * self.k } }
func via<T: Shout>(t: &T) -> u64 { t.shout("four") }
func via_dyn(t: &dyn Shout) -> u64 { t.shout("fives") }
type Text = string
func aliased<T>(x: T, s: &Text) -> u64 { s.len() }

func main() -> i32 {
    val boxed: Pair<&string> = Pair { v: "boxed" }
    val loud = Loud { k: 2 }
    val a = id::<string>("gen")
    val total = tagged(1, "four") + Pair { v: 2 }.label("xy") + via(&loud) + via_dyn(&loud)
    val al = aliased(true, "alias")
    println("{a} {boxed.v} {total} {al}")
    0
}
"#,
    );
    // 4 + 102 + 8 + 10.
    assert_eq!(out, "gen boxed 124 5\n");
}

/// The view computed four equivalent ways: a tail expression, a bound then returned value,
/// a nested branch, and through a generic and a closure, each read as a call argument.
#[test]
fn equivalent_forms_of_a_literal_view_agree() {
    let exit = leak_checked(
        "literal_forms.nr",
        r#"
func len_of(s: &string) -> u64 { s.len() }
func tail() -> &string { "tail!" }
func bound() -> &string {
    val v: &string = "tail!"
    return v
}
func nested(n: i32) -> &string {
    if n > 0 {
        match n { 1 => "tail!", _ => "other" }
    } else {
        "never"
    }
}
func through<T>(x: &T) -> &T { x }

func main() -> i32 {
    val call = |s: &string| -> u64 { len_of(s) }
    mut total: u64 = 0
    mut i = 0
    while i < 5 {
        val owned = "own" + "ed"
        total = total + len_of(tail()) + len_of(bound()) + len_of(nested(1))
        total = total + len_of(through::<string>("tail!")) + call("tail!") + len_of(&owned)
        i += 1
    }
    total as i32
}
"#,
    );
    // Five views of "tail!" and "owned", five times: 5 * (5 * 5 + 5).
    assert_eq!(exit, 150);
}

#[test]
fn a_literal_held_beside_owned_strings_releases_only_the_owners() {
    let exit = leak_checked(
        "literal_beside_owners.nr",
        r#"
struct Named { name: &string, owned: string }
func count(s: &string) -> u64 { s.len() }
func wrap(s: &string) -> Option<&string> { Some(s) }

func main() -> i32 {
    mut total: u64 = 0
    mut i = 0
    while i < 10 {
        val n = Named { name: "lit", owned: "own" + "ed" }
        mut r: &string = "first"
        r = "second"
        val joined = r + "!"
        val o = wrap("wrapped") ?? "none"
        total = total + count(n.name) + count(&n.owned) + count(o) + count(&joined)
        i += 1
    }
    total as i32
}
"#,
    );
    // 10 * (3 + 5 + 7 + 7).
    assert_eq!(exit, 220);
}

#[test]
fn a_literal_is_not_a_mutable_or_double_borrow() {
    let err = rejected(
        "literal_not_mut.nr",
        r#"
func m(s: &mut string) { }
func main() -> i32 {
    val x: &mut string = "a"
    m("b")
    val y: &&string = "c"
    0
}
"#,
    );
    assert!(err.contains("expected &mut string, found string"), "{err}");
    assert!(err.contains("expected &&string, found string"), "{err}");
}

#[test]
fn an_unbound_generic_reference_does_not_take_a_literal() {
    let err = rejected(
        "literal_open_generic.nr",
        r#"
func id<T>(x: &T) -> &T { x }
func main() -> i32 {
    val a = id("gen")
    0
}
"#,
    );
    assert!(err.contains("expected &T, found string"), "{err}");
}

#[test]
fn an_owned_string_value_is_still_borrowed_explicitly() {
    let err = rejected(
        "owned_not_implicit.nr",
        r#"
func len_of(s: &string) -> u64 { s.len() }
func main() -> i32 {
    val s = "a" + "b"
    val n = len_of(s)
    val m = len_of("built {n}")
    0
}
"#,
    );
    // An interpolated literal is built fresh and owns its buffer, so it is no view either.
    assert_eq!(
        err.matches("expected &string, found string").count(),
        2,
        "{err}"
    );
}
