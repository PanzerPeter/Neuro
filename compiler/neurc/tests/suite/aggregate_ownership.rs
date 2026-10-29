//! Non-`Copy` values held in aggregates: arrays, tuples, enum payloads, newtypes, and
//! generic struct or enum type arguments.
//!
//! The aggregate becomes the owner, so the rules under test are ownership rules rather
//! than layout ones: the aggregate moves when it holds a move-tracked value, a sub-place
//! gives up one element at a time, and reading the aggregate as a whole afterwards is a
//! use of a partially moved value.
use crate::compile_harness::CompileTest;

#[test]
fn an_array_of_strings_round_trips_through_codegen() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val names: [string; 3] = ["alpha", "beta", "gamma"]
    println(names[1])
    names[2].len() as i32
}
"#;
    let exit = test
        .compile_and_run("agg_array_strings.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 5);
}

#[test]
fn a_tuple_carries_a_string_out_of_a_function() {
    let test = CompileTest::new();
    let source = r#"
func split(head: string) -> (string, i32) {
    (head, 7)
}

func main() -> i32 {
    val pair = split("head")
    println(pair.0)
    pair.1
}
"#;
    let exit = test
        .compile_and_run("agg_tuple_return.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 7);
}

#[test]
fn every_aggregate_shape_destructures_element_by_element() {
    // The destructure desugar binds one projection per leaf, so a rule that collapsed
    // each of them onto the root binding rejected the second leaf of every pattern.
    let test = CompileTest::new();
    let source = r#"
struct Two { a: string, b: string }

func main() -> i32 {
    val Two { a, b } = Two { a: "x", b: "yy" }
    val (l, r) = ("p", "qqq")
    val [i, j] = ["m", "nnnn"]
    (a.len() + b.len() + l.len() + r.len() + i.len() + j.len()) as i32
}
"#;
    let exit = test
        .compile_and_run("agg_destructure.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 12);
}

#[test]
fn reading_an_aggregate_whole_after_a_partial_move_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
struct Two { a: string, b: string }

func take(t: Two) -> i32 { 0 }

func main() -> i32 {
    val t = Two { a: "x", b: "y" }
    val a = t.a
    take(t)
}
"#;
    let path = test.write_source("agg_partial_move.nr", source);
    let err = test
        .compile(&path)
        .expect_err("a partially moved value must not be passed on");
    assert!(
        err.contains("use of moved value 't'"),
        "expected a moved-value diagnostic naming the root, got: {err}"
    );
}

#[test]
fn taking_the_same_element_twice_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val pair: (string, string) = ("p", "q")
    val first = pair.0
    val again = pair.0
    0
}
"#;
    let path = test.write_source("agg_same_element.nr", source);
    let err = test
        .compile(&path)
        .expect_err("one element may be given away once, not twice");
    assert!(err.contains("use of moved value 'pair'"), "{err}");
}

#[test]
fn an_element_taken_by_a_runtime_index_moves_the_whole_array() {
    // The compiler cannot say which element `a[i]` gave away, so the conservative
    // answer is the only sound one.
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val names: [string; 2] = ["a", "b"]
    mut i = 0
    val taken = names[i]
    println(names[1])
    0
}
"#;
    let path = test.write_source("agg_runtime_index.nr", source);
    let err = test
        .compile(&path)
        .expect_err("a move through a runtime index must take the whole binding");
    assert!(err.contains("use of moved value 'names'"), "{err}");
}

#[test]
fn an_enum_payload_holds_a_string_a_struct_and_an_array() {
    let test = CompileTest::new();
    let source = r#"
struct Point { x: i32, y: i32 }

enum Cell {
    Text(string),
    At(Point),
    Row([string; 2]),
    Empty,
}

func weigh(c: Cell) -> i32 {
    match c {
        Cell::Text(t) => t.len() as i32
        Cell::At(p) => p.x + p.y
        Cell::Row(r) => r[1].len() as i32
        Cell::Empty => 0
    }
}

func main() -> i32 {
    weigh(Cell::Text("abcd"))
        + weigh(Cell::At(Point { x: 3, y: 4 }))
        + weigh(Cell::Row(["a", "bbb"]))
        + weigh(Cell::Empty)
}
"#;
    let exit = test
        .compile_and_run("agg_enum_payload.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 14);
}

#[test]
fn an_enum_payload_may_name_a_struct_declared_after_it() {
    // Enum names are registered before structs and enum payloads after, so neither
    // declaration order is the special case.
    let test = CompileTest::new();
    let source = r#"
enum Holder { At(Point), Empty }

struct Point { x: i32, y: i32 }

func main() -> i32 {
    match Holder::At(Point { x: 2, y: 3 }) {
        Holder::At(p) => p.x * p.y
        Holder::Empty => 0
    }
}
"#;
    let exit = test
        .compile_and_run("agg_enum_forward_struct.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 6);
}

#[test]
fn option_and_vec_carry_a_string_payload() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut words: Vec<string> = Vec::new()
    words.push("alpha")
    words.push("bravo")
    match words.pop() {
        Option::Some(w) => println(w)
        Option::None => println("empty")
    }
    words.len() as i32
}
"#;
    let exit = test
        .compile_and_run("agg_option_string.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 1);
}

#[test]
fn a_newtype_wraps_a_string_and_moves_with_it() {
    let test = CompileTest::new();
    let source = r#"
newtype Name = string

func take(n: Name) -> i32 {
    n.0.len() as i32
}

func main() -> i32 {
    val n = Name("ada")
    take(n)
}
"#;
    let exit = test
        .compile_and_run("agg_newtype.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 3);

    let reused = r#"
newtype Name = string

func take(n: Name) -> i32 { 0 }

func main() -> i32 {
    val n = Name("ada")
    val first = take(n)
    take(n)
}
"#;
    let path = test.write_source("agg_newtype_moved.nr", reused);
    assert!(
        test.compile(&path).is_err(),
        "a newtype over a non-Copy inner must move, not copy"
    );
}

#[test]
fn a_copy_derive_still_rejects_an_aggregate_field_that_owns() {
    let test = CompileTest::new();
    let source = r#"
@derive(Copy, Clone)
struct Bad { names: [string; 2] }

func main() -> i32 { 0 }
"#;
    let path = test.write_source("agg_copy_derive.nr", source);
    let err = test
        .compile(&path)
        .expect_err("an owning array field must block `@derive(Copy)`");
    assert!(
        err.contains("cannot be `Copy`"),
        "expected a derive diagnostic, got: {err}"
    );
}

/// BUG-052: a tuple literal left each owned element armed in the binding it was moved
/// out of, so the binding and the tuple's holder both released it. Every heap-owning
/// kind double-freed; returning the tuple from a function aborted the program.
#[test]
fn regression_bug_052_a_tuple_literal_takes_ownership_of_its_elements() {
    let test = CompileTest::new();
    let source = r#"
func pair() -> (Tensor<f32, [2]>, Vec<i32>) {
    val weights: Tensor<f32, [2]> = [1.5, 2.5]
    mut counts: Vec<i32> = Vec::new()
    counts.push(4)
    return (weights, counts)
}

func main() -> i32 {
    val result = pair()
    val local: Tensor<f32, [2]> = [1.0, 1.0]
    val held = (local, 3)
    (result.0.sum() + held.0.sum()) as i32 + result.1[0] + held.1
}
"#;
    let exit = test
        .compile_and_run("bug_052_tuple.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 13);
}

/// BUG-052, the array half, observed through a destructor: a value moved into an array
/// literal was dropped by its old binding and again by the array.
#[test]
fn regression_bug_052_an_array_literal_takes_ownership_of_its_elements() {
    let test = CompileTest::new();
    let source = r#"
struct Noisy { id: i32 }

impl Drop for Noisy {
    func drop(&mut self) {
        println("drop {self.id}")
    }
}

func main() -> i32 {
    val first = Noisy { id: 1 }
    val tuple = (Noisy { id: 2 }, 0)
    val held = [first]
    held[0].id + tuple.1
}
"#;
    let exe = test
        .compile(&test.write_source("bug_052_array.nr", source))
        .expect("compile failed");
    let run = std::process::Command::new(&exe)
        .output()
        .expect("run executable");
    assert_eq!(run.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&run.stdout);
    for id in [1, 2] {
        assert_eq!(
            stdout.matches(&format!("drop {id}")).count(),
            1,
            "value {id} must be destroyed exactly once, got:\n{stdout}"
        );
    }
}

/// Run `source` with its address space capped at 512 MB, so a loop that leaks a megabyte
/// per iteration runs out of memory where one that releases it stays flat.
#[cfg(target_os = "linux")]
fn exit_under_memory_cap(name: &str, source: &str) -> Option<i32> {
    let test = CompileTest::new();
    let exe = test
        .compile(&test.write_source(name, source))
        .expect("compile failed");
    std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("ulimit -v 524288; exec '{}'", exe.display()))
        .status()
        .expect("run under a memory cap")
        .code()
}

/// A `string` moved out of a held position (`val taken = m.label`) takes that position's
/// ownership with it, and a `move` closure owning a struct releases its `string` fields:
/// each loop below leaked a megabyte an iteration.
#[cfg(target_os = "linux")]
#[test]
fn a_string_moved_out_of_a_holder_or_a_captured_struct_is_released() {
    let source = r#"
struct N { id: i32, label: string }
func main() -> i32 {
    mut p = "0123456789abcdef"
    for k in 0..16 { p = p + p }
    mut total: i64 = 0
    for i in 0..1000 {
        val n = N { id: 1, label: p + "" }
        val f = move || -> i32 { n.id }
        total += f() as i64
        val m = N { id: 2, label: p + "" }
        val taken = m.label
        total += taken.len() as i64
    }
    if total == 1000 + 1000 * 1048576 { 0 } else { 1 }
}
"#;
    assert_eq!(
        exit_under_memory_cap("held_string_moves.nr", source),
        Some(0)
    );
}

/// A `string` parameter handed to a method that only reads it is read only itself, so
/// the caller of the outer function releases the argument after the call.
#[cfg(target_os = "linux")]
#[test]
fn a_parameter_passed_to_a_reading_method_is_released_by_the_caller() {
    let source = r#"
struct Log { n: i32 }
impl Log {
    func say(&self, s: string) -> i64 { s.len() as i64 + self.n as i64 }
}
func relay(l: &Log, s: string) -> i64 { l.say(s) }
func main() -> i32 {
    mut p = "0123456789abcdef"
    for k in 0..16 { p = p + p }
    val l = Log { n: 1 }
    mut total: i64 = 0
    for i in 0..1000 {
        total += relay(&l, p + "")
    }
    if total == 1000 * (1048576 + 1) { 0 } else { 1 }
}
"#;
    assert_eq!(exit_under_memory_cap("reading_method.nr", source), Some(0));
}

/// A `return` written inside an expression (here a `match` arm) is one more exit of the
/// function: when every exit allocates, the caller owns and releases the result.
#[cfg(target_os = "linux")]
#[test]
fn a_return_inside_an_expression_still_makes_a_producer() {
    let source = r#"
func pick(p: &string, n: i32) -> string {
    val y = match n {
        0 => { return p + "" },
        _ => n,
    }
    p + "{y}"
}
func main() -> i32 {
    mut p = "0123456789abcdef"
    for k in 0..16 { p = p + p }
    mut total: i64 = 0
    for i in 0..1000 {
        total += pick(&p, i % 2).len() as i64
    }
    if total == 1000 * 1048576 + 500 { 0 } else { 1 }
}
"#;
    assert_eq!(exit_under_memory_cap("nested_return.nr", source), Some(0));
}

/// A function whose tail is a `loop` leaves through its `break` values (and any `return`
/// inside): when every one allocates, the caller owns and releases the result. A literal
/// `break` beside an allocating one is copied, so the caller never frees `.rodata`.
#[cfg(target_os = "linux")]
#[test]
fn a_loop_tail_with_allocating_breaks_is_a_producer() {
    let source = r#"
func plain(a: &string, c: bool) -> string {
    loop {
        if c { return a + "" }
        break a + "!"
    }
}
func labeled(a: &string, c: bool) -> string {
    outer: loop {
        loop {
            if c { break outer a + "?" }
            break
        }
        break outer a + "!"
    }
}
func mixed(a: &string, c: bool) -> string {
    loop {
        if c { break "none" }
        break a + "!"
    }
}
func main() -> i32 {
    mut p = "0123456789abcdef"
    for k in 0..16 { p = p + p }
    mut total: i64 = 0
    for i in 0..1000 {
        val c = i % 2 == 0
        total += plain(&p, c).len() as i64
        total += labeled(&p, c).len() as i64
        total += mixed(&p, c).len() as i64
    }
    if total == 1048577 * 2000 + 1048576 * 500 + 2000 { 0 } else { 1 }
}
"#;
    assert_eq!(
        exit_under_memory_cap("loop_tail_breaks.nr", source),
        Some(0)
    );
}

/// A `break` written in a `match` arm of a tail `loop` is still one of its exits.
#[cfg(target_os = "linux")]
#[test]
fn a_break_in_a_match_arm_of_a_loop_tail_is_an_exit() {
    let source = r#"
func matched(a: &string, n: i32) -> string {
    loop {
        match n {
            0 => { break "zero" }
            _ => { break a + "m" }
        }
    }
}
func main() -> i32 {
    mut p = "0123456789abcdef"
    for k in 0..16 { p = p + p }
    mut total: i64 = 0
    for i in 0..1000 {
        total += matched(&p, i % 2).len() as i64
    }
    if total == 500 * 4 + 500 * 1048577 { 0 } else { 1 }
}
"#;
    assert_eq!(exit_under_memory_cap("loop_tail_match.nr", source), Some(0));
}

/// A holder an `if`, an `else if` chain, a nested `if`, a `match` or a block built owns
/// the `string` positions its taken arm allocated or moved a binding into, so each loop
/// below stays flat. A literal arm owns nothing, and a diverging arm hands over nothing.
#[cfg(target_os = "linux")]
#[test]
fn a_holder_a_branch_built_owns_its_string_positions() {
    let source = r#"
struct P { name: string, id: i32 }
struct O { inner: P, tag: string }
func pick(c: bool, p: &string) -> i64 {
    val e = if c { P { name: p + "", id: 1 } } else { return 0 }
    e.name.len() as i64
}
func main() -> i32 {
    mut p = "0123456789abcdef"
    for k in 0..16 { p = p + p }
    mut total: i64 = 0
    for i in 0..400 {
        val n = i % 3
        val flat = if n == 0 { P { name: p + "", id: 1 } } else { P { name: "lit", id: 2 } }
        val chain = if n == 0 { P { name: "a", id: 0 } } else if n == 1 { P { name: p + "", id: 1 } } else { if n == 2 { P { name: p + "!", id: 2 } } else { P { name: "z", id: 3 } } }
        val t = p + ""
        val picked = match n {
            0 => P { name: t, id: 0 },
            1 => {
                val q = P { name: p + "", id: 1 }
                q
            },
            _ => P { name: "x", id: 2 },
        }
        val pair = if n == 1 { (p + "", 1) } else { ("lit", 2) }
        val deep = if n == 2 { O { inner: P { name: p + "", id: 1 }, tag: p + "" } } else { O { inner: P { name: "i", id: 0 }, tag: "t" } }
        val src = P { name: p + "", id: 5 }
        val other = P { name: "o", id: 6 }
        val via = if n == 0 { src } else { other }
        val moved = via
        val blk = {
            val s = p + ""
            P { name: s, id: 7 }
        }
        mut r = P { name: "r", id: 0 }
        r = if n == 1 { P { name: p + "", id: 1 } } else { P { name: "s", id: 2 } }
        total += (flat.name.len() + chain.name.len() + picked.name.len() + pair.0.len()) as i64
        total += (deep.inner.name.len() + deep.tag.len() + moved.name.len() + blk.name.len()) as i64
        total += r.name.len() as i64 + pick(n == 2, &p)
    }
    if total == 1866 * 1048576 + 3066 { 0 } else { 1 }
}
"#;
    assert_eq!(exit_under_memory_cap("branch_holder.nr", source), Some(0));
}

/// A holder a call built owns the `string` positions its callee allocated on every return
/// path: a struct, a nested struct, a tuple, a method's result and a returned local. A
/// position some path fills from a literal stays unowned rather than freeing `.rodata`.
#[cfg(target_os = "linux")]
#[test]
fn a_holder_a_call_built_owns_the_positions_its_callee_allocated() {
    let source = r#"
struct P { name: string, id: i32 }
struct O { inner: P, tag: string }
struct M { id: i32 }
impl M {
    func build(&self, a: &string) -> P { P { name: a + "!", id: self.id } }
}
func make(a: &string) -> P { P { name: a + "", id: 1 } }
func via(a: &string) -> O { O { inner: make(a), tag: a + "" } }
func pair(c: bool, a: &string) -> (string, string) {
    if c { return (a + "?", "lit") }
    (a + "", "x")
}
func local(a: &string) -> P {
    val q = P { name: a + "", id: 4 }
    q
}
func main() -> i32 {
    mut p = "0123456789abcdef"
    for k in 0..16 { p = p + p }
    val m = M { id: 3 }
    mut total: i64 = 0
    for i in 0..200 {
        val even = i % 2 == 0
        val e1 = make(&p)
        val e2 = via(&p)
        val e3 = pair(even, &p)
        val e4 = m.build(&p)
        val e5 = local(&p)
        mut e6 = make(&p)
        e6 = make(&p)
        val e7 = if even { make(&p) } else { P { name: "n", id: 0 } }
        total += (e1.name.len() + e2.inner.name.len() + e2.tag.len() + e3.0.len()) as i64
        total += (e4.name.len() + e5.name.len() + e6.name.len() + e7.name.len()) as i64
    }
    if total == 1500 * 1048576 + 400 { 0 } else { 1 }
}
"#;
    assert_eq!(exit_under_memory_cap("call_holder.nr", source), Some(0));
}

/// BUG-133: a store into a moved-out field, tuple element or literal array element
/// refills it, and the holder may be read whole again once every place that left is
/// back. Each round below moves a megabyte out of every position and stores a fresh one
/// in, so a store that leaked the incoming value or released the moved-out one twice
/// would run out of memory or abort.
#[cfg(target_os = "linux")]
#[test]
fn regression_bug_133_a_store_refills_a_moved_out_place() {
    let source = r#"
struct H { s: string, v: Vec<i32>, n: i32 }
struct Outer { inner: H, tag: string }
func take(v: Vec<i32>) -> i64 { v.len() as i64 }
func main() -> i32 {
    mut p = "0123456789abcdef"
    for k in 0..16 { p = p + p }
    mut h = H { s: p + "", v: Vec::new(), n: 1 }
    mut t: (string, i32) = (p + "", 2)
    mut a: [string; 2] = [p + "", p + ""]
    mut o = Outer { inner: H { s: p + "", v: Vec::new(), n: 3 }, tag: p + "" }
    mut total: i64 = 0
    for i in 0..200 {
        val s = h.s
        val v = h.v
        total += s.len() as i64 + take(v)
        h.s = p + ""
        h.v = Vec::new()
        h.v.push(i)
        val e = t.0
        total += e.len() as i64
        t.0 = p + ""
        val first = a[0]
        total += first.len() as i64
        a[0] = p + ""
        val inner = o.inner
        total += inner.s.len() as i64
        if i % 2 == 0 { o.inner = inner } else { o.inner = H { s: p + "", v: Vec::new(), n: 4 } }
    }
    val whole = h
    val pair = t
    val held = a
    val outer = o
    total += whole.v.len() as i64 + pair.0.len() as i64 + held[1].len() as i64
    total += outer.tag.len() as i64
    if total == 803 * 1048576 + 200 { 0 } else { 1 }
}
"#;
    assert_eq!(exit_under_memory_cap("bug_133_refill.nr", source), Some(0));
}

/// BUG-133, through a destructor: a `Drop` value moved out of a field and replaced is
/// destroyed once by whoever took it, and the replacement once by the holder.
#[test]
fn regression_bug_133_a_refilled_drop_field_is_destroyed_once() {
    let test = CompileTest::new();
    let source = r#"
struct Noisy { id: i32 }

impl Drop for Noisy {
    func drop(&mut self) {
        println("drop {self.id}")
    }
}

struct Pair { left: Noisy, right: Noisy }

func main() -> i32 {
    mut p = Pair { left: Noisy { id: 1 }, right: Noisy { id: 2 } }
    val taken = p.left
    p.left = Noisy { id: 3 }
    val whole = p
    taken.id + whole.left.id
}
"#;
    let exe = test
        .compile(&test.write_source("bug_133_drop.nr", source))
        .expect("compile failed");
    let run = std::process::Command::new(&exe)
        .output()
        .expect("run executable");
    assert_eq!(run.status.code(), Some(4));
    let stdout = String::from_utf8_lossy(&run.stdout);
    for id in [1, 2, 3] {
        assert_eq!(
            stdout.matches(&format!("drop {id}")).count(),
            1,
            "value {id} must be destroyed exactly once, got:\n{stdout}"
        );
    }
}

/// BUG-133: a place refilled on only one path out of an `if` may still be moved where the
/// paths meet, and a place whose holder or ancestor left with it has nothing to refill.
#[test]
fn regression_bug_133_a_refill_that_may_not_have_run_is_rejected() {
    let test = CompileTest::new();
    let cases = [
        (
            "one_branch",
            r#"
struct P { name: string, n: i32 }
func main() -> i32 {
    val k: string = "x"
    mut p = P { name: k + "a", n: 1 }
    val s = p.name
    if s.len() > 1 { p.name = k + "b" }
    val q = p
    0
}
"#,
        ),
        (
            "whole_moved",
            r#"
struct P { name: string, n: i32 }
func main() -> i32 {
    val k: string = "x"
    mut p = P { name: k + "a", n: 1 }
    val q = p
    p.name = k + "b"
    0
}
"#,
        ),
        (
            "ancestor_moved",
            r#"
struct I { name: string }
struct P { inner: I, n: i32 }
func main() -> i32 {
    val k: string = "x"
    mut p = P { inner: I { name: k + "a" }, n: 1 }
    val q = p.inner
    p.inner.name = k + "b"
    0
}
"#,
        ),
        (
            "compound",
            r#"
struct P { name: string, n: i32 }
func main() -> i32 {
    val k: string = "x"
    mut p = P { name: k + "a", n: 1 }
    val s = p.name
    p.name += "z"
    0
}
"#,
        ),
    ];
    for (name, source) in cases {
        let error = test
            .check(&format!("bug_133_{name}.nr"), source)
            .expect_err(name);
        assert!(error.contains("use of moved value 'p'"), "{name}: {error}");
    }
}

/// BUG-131: a `string` binding stored into a field, a tuple element or an array element
/// (at a literal or a run-time position) hands its ownership to the position. Each store
/// below leaked a megabyte.
#[cfg(target_os = "linux")]
#[test]
fn regression_bug_131_a_string_binding_stored_into_a_position_is_released() {
    let source = r#"
struct Entry { label: string, weight: i32 }
func main() -> i32 {
    mut p = "0123456789abcdef"
    for k in 0..16 { p = p + p }
    mut total: i64 = 0
    for i in 0..200 {
        mut e = Entry { label: "a", weight: 1 }
        val s = p + ""
        e.label = s
        mut t: (string, i32) = ("b", 2)
        val u = p + ""
        t.0 = u
        mut a: [string; 2] = ["c", "d"]
        val w = p + ""
        a[0] = w
        val z = p + ""
        a[i % 2] = z
        total += (e.label.len() + t.0.len() + a[0].len()) as i64
    }
    if total == 600 * 1048576 { 0 } else { 1 }
}
"#;
    assert_eq!(
        exit_under_memory_cap("bug_131_position.nr", source),
        Some(0)
    );
}

/// A `string` an `if`, `match` or block builds owns its buffer when the arm that ran
/// built one, and owns nothing when that arm yields a literal: mixing the two used to
/// leave the binding owning nothing, and each owned arm leaked a megabyte.
#[cfg(target_os = "linux")]
#[test]
fn a_string_built_by_a_branch_with_a_literal_arm_is_released() {
    let source = r#"
func main() -> i32 {
    mut p = "0123456789abcdef"
    for k in 0..16 { p = p + p }
    mut total: i64 = 0
    for i in 0..1000 {
        val held: string = p + ""
        val a: string = if i % 2 == 0 { p + "" } else { "lit" }
        val b = match i % 3 {
            0 => held,
            _ => "m",
        }
        mut c: string = "start"
        c = if i % 2 == 1 { p + "" } else { "lit" }
        total += (a.len() + b.len() + c.len()) as i64
    }
    if total == 1398804050 { 0 } else { 1 }
}
"#;
    assert_eq!(exit_under_memory_cap("branch_string.nr", source), Some(0));
}

/// A holder moved out of a field (`val inner = o.inner`) takes the ownership of its
/// `string` positions with it, as a holder moved out of a binding does; each move leaked
/// a megabyte.
#[cfg(target_os = "linux")]
#[test]
fn a_holder_moved_out_of_a_field_releases_its_strings() {
    let source = r#"
struct H { s: string, n: i32 }
struct Outer { inner: H, tag: i32 }
func main() -> i32 {
    mut p = "0123456789abcdef"
    for k in 0..16 { p = p + p }
    mut total: i64 = 0
    for i in 0..1000 {
        val o = Outer { inner: H { s: p + "", n: 1 }, tag: 2 }
        val inner = o.inner
        total += inner.s.len() as i64
    }
    if total == 1000 * 1048576 { 0 } else { 1 }
}
"#;
    assert_eq!(
        exit_under_memory_cap("held_holder_move.nr", source),
        Some(0)
    );
}
