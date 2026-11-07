//! Closure capture modes: a capture is copied, borrowed `&` or `&mut`, or moved, as the
//! body uses it; the strongest use makes the closure `Fn`, `FnMut` or `FnOnce`, and a
//! function type's `mut` / `once` mode says which of them a parameter takes.

use crate::compile_harness::{rejected, run, stdout_of};

#[test]
fn a_closure_reading_a_string_borrows_it_and_leaves_it_usable() {
    let source = r#"
func apply(f: (i32) -> i32, v: i32) -> i32 { f(v) }
func main() -> i32 {
    val greeting = "hello" + " world"
    val n = {
        val greet = |extra: i32| -> i32 { (greeting.len() as i32) + extra }
        apply(greet, 1) + greet(2)
    }
    return n + (greeting.len() as i32)
}
"#;
    assert_eq!(run("borrowed_read.nr", source), 12 + 13 + 11);
}

#[test]
fn a_closure_writing_a_capture_borrows_it_mutably() {
    let source = r#"
func twice(f: mut () -> void) {
    f()
    f()
}
func main() -> i32 {
    mut count = 0
    mut log: Vec<i32> = Vec::new()
    {
        val bump = || -> void { count += 1 }
        bump()
        twice(bump)
        val record = |x: i32| -> void { log.push(x * 10) }
        record(4)
        record(5)
    }
    return count * 20 + (log.len() as i32) * 5 + log[1] / 10
}
"#;
    assert_eq!(run("mutable_capture.nr", source), 75);
}

#[test]
fn a_move_closure_keeps_its_own_copy_between_calls() {
    let source = r#"
func main() -> i32 {
    mut n = 0
    val tick = move || -> i32 {
        n += 1
        n
    }
    val a = tick()
    val b = tick()
    return a * 10 + b + n * 100
}
"#;
    assert_eq!(run("move_counter.nr", source), 12);
}

#[test]
fn a_consuming_move_closure_runs_once_through_a_once_parameter() {
    let source = r#"
func consume(s: string) -> u64 { s.len() }
func run(f: once () -> u64) -> u64 { f() }
func main() -> i32 {
    mut total: u64 = 0
    for i in 0..3 {
        val word = "ab" + "cd"
        total += run(move || -> u64 { consume(word) })
        val unused = "never" + "run"
        val skipped = move || -> u64 { consume(unused) }
    }
    return total as i32
}
"#;
    assert_eq!(run("once_closure.nr", source), 12);
}

#[test]
fn a_mut_closure_is_refused_where_a_plain_function_is_asked() {
    let err = rejected(
        "fnmut_to_fn.nr",
        r#"
func apply(f: () -> void) { f() }
func main() -> i32 {
    mut c = 0
    apply(|| -> void { c += 1 })
    return c
}
"#,
    );
    assert!(err.contains("found mut fn() -> void"), "{err}");
}

#[test]
fn a_once_closure_is_refused_where_a_mut_function_is_asked() {
    let err = rejected(
        "once_to_mut.nr",
        r#"
func consume(s: string) -> u64 { s.len() }
func twice(f: mut () -> u64) -> u64 { f() + f() }
func main() -> i32 {
    val s = "a" + "b"
    return twice(move || -> u64 { consume(s) }) as i32
}
"#,
    );
    assert!(err.contains("found once fn() -> u64"), "{err}");
}

#[test]
fn a_once_value_cannot_be_called_twice() {
    let err = rejected(
        "once_twice.nr",
        r#"
func run(f: once () -> u64) -> u64 { f() + f() }
func main() -> i32 { return 0 }
"#,
    );
    assert!(err.contains("use of moved value 'f'"), "{err}");
}

/// Shared by the tests below: a `Drop` type whose release prints its id, so a stdout pin
/// shows each value is released exactly once, and functions that consume a value.
const OWNERS: &str = r#"
struct Noisy { id: i32 }
impl Drop for Noisy {
    func drop(&mut self) { println("drop {self.id}") }
}
struct Pair { name: string, age: i32 }
func consume(s: string) -> u64 { s.len() }
func eat(n: Noisy) -> i32 { n.id }
func call(f: once () -> u64) -> u64 { f() }
"#;

fn with_owners(main: &str) -> String {
    format!("{OWNERS}\n{main}")
}

#[test]
fn a_closure_without_move_takes_what_its_body_moves_out() {
    let source = with_owners(
        r#"
func main() -> i32 {
    val s = "a" + "b"
    val f = || -> u64 { consume(s) }
    return f() as i32
}
"#,
    );
    assert_eq!(run("inferred_move.nr", &source), 2);
}

#[test]
fn an_inferred_capture_is_settled_per_variable() {
    let source = with_owners(
        r#"
func main() -> i32 {
    val kept = "abc" + "d"
    val s = "a" + "b"
    val n = {
        val f = || -> u64 { consume(s) + kept.len() }
        f()
    }
    return (n + kept.len()) as i32
}
"#,
    );
    assert_eq!(run("inferred_per_variable.nr", &source), 10);
}

#[test]
fn an_inferred_capture_is_gone_from_its_source() {
    let err = rejected(
        "inferred_gone.nr",
        &with_owners(
            r#"
func main() -> i32 {
    val s = "a" + "b"
    val f = || -> u64 { consume(s) }
    println(s)
    return f() as i32
}
"#,
        ),
    );
    assert!(err.contains("use of moved value 's'"), "{err}");
}

#[test]
fn a_part_moved_out_takes_the_whole_capture() {
    let err = rejected(
        "inferred_part.nr",
        &with_owners(
            r#"
func main() -> i32 {
    val p = Pair { name: "a" + "b", age: 3 }
    val f = || -> u64 { consume(p.name) }
    println("{p.age}")
    return f() as i32
}
"#,
        ),
    );
    assert!(err.contains("use of moved value 'p'"), "{err}");
}

#[test]
fn an_inferred_consumer_runs_once() {
    for (name, body) in [
        ("twice", "val a = f()\n    return (a + f()) as i32"),
        ("plain_slot", "func_slot(f)\n    return 0"),
    ] {
        let source = with_owners(&format!(
            r#"
func func_slot(g: () -> u64) -> u64 {{ g() }}
func main() -> i32 {{
    val s = "a" + "b"
    val f = || -> u64 {{ consume(s) }}
    {body}
}}
"#
        ));
        rejected(&format!("inferred_{name}.nr"), &source);
    }
    let source = with_owners(
        r#"
func main() -> i32 {
    val s = "a" + "b"
    return call(|| -> u64 { consume(s) }) as i32
}
"#,
    );
    assert_eq!(run("inferred_once_slot.nr", &source), 2);
}

/// A closure that only moves its captures owns them, so nothing ties it to the scope
/// that made it, and each value is released once whether or not it ever ran.
#[test]
fn an_owning_inferred_closure_escapes_and_releases_once() {
    let source = with_owners(
        r#"
struct Holder { run: once () -> u64 }
func make(s: string) -> once () -> u64 { || consume(s) }
func eaten(n: Noisy) -> once () -> i32 { || eat(n) }
func main() -> i32 {
    val held = Holder { run: make("abc") }
    val never = eaten(Noisy { id: 1 })
    val ran = eaten(Noisy { id: 2 })
    val a = ran()
    println("done")
    return a
}
"#,
    );
    let test = crate::compile_harness::CompileTest::new();
    assert_eq!(test.run_leak_checked("inferred_escapes.nr", &source), 2);
    assert_eq!(
        stdout_of("inferred_escapes_out.nr", &source),
        "drop 2\ndone\ndrop 1\n"
    );
}

/// A closure's result leaves it, so a capture it ends with is given away whatever shape
/// the tail takes. Read as a borrow, a second call yielded the same value twice.
#[test]
fn a_capture_a_closure_ends_with_is_moved_out() {
    for (name, literal) in [
        ("bare", "|| n"),
        ("block", "|| { n }"),
        ("branch", "|| { if flag { n } else { Noisy { id: 2 } } }"),
        ("written", "move || n"),
    ] {
        let once = with_owners(&format!(
            r#"
func main() -> i32 {{
    val flag = true
    val n = Noisy {{ id: 1 }}
    val f = {literal}
    val got = f()
    return got.id
}}
"#
        ));
        let test = crate::compile_harness::CompileTest::new();
        let file = format!("tail_{name}.nr");
        assert_eq!(test.run_leak_checked(&file, &once), 1, "{literal}");
        assert_eq!(stdout_of(&file, &once), "drop 1\n", "{literal}");

        let twice = once.replace("return got.id", "val again = f()\n    return got.id");
        let err = rejected(&format!("tail_twice_{name}.nr"), &twice);
        assert!(err.contains("use of moved value 'f'"), "{literal}: {err}");
    }
}

/// A closure's own parameter or local at the tail is not a capture, and a `Copy` capture
/// is copied, so none of these uses anything up.
#[test]
fn a_tail_that_moves_no_capture_leaves_the_closure_callable() {
    let source = with_owners(
        r#"
func main() -> i32 {
    val base = 4
    val same = |s: string| s
    val fresh = || {
        val s = "a" + "b"
        s
    }
    val copied = || base
    val a = same("abc" + "d").len()
    val b = same(fresh()).len()
    return (a + b) as i32 + copied() + copied()
}
"#,
    );
    assert_eq!(run("tail_no_capture.nr", &source), 14);
}

#[test]
fn a_collection_element_cannot_leave_as_a_closure_result() {
    let err = rejected(
        "tail_element.nr",
        &with_owners(
            r#"
func main() -> i32 {
    mut v: Vec<string> = Vec::new()
    v.push("a" + "b")
    val f = || v[0]
    return f().len() as i32
}
"#,
        ),
    );
    assert!(err.contains("cannot move"), "{err}");
}

/// The caller used to release a temporary argument on return, with the returned closure
/// still holding it, which only a read of the bytes could show.
#[test]
fn a_returned_closure_keeps_the_argument_it_captured() {
    for head in ["||", "move ||"] {
        let source = format!(
            r#"
func later(item: string) -> once () -> string {{ {head} item }}
func main() -> i32 {{
    val filler = "zzzz" + "zzzz"
    val f = later("ab" + "cd")
    val other = "yy" + "yy"
    println(f())
    return (filler.len() + other.len()) as i32
}}
"#
        );
        assert_eq!(stdout_of("kept_argument.nr", &source), "abcd\n");
    }
}

#[test]
fn a_nested_move_takes_the_capture_at_every_level() {
    for (name, inner) in [("inferred", "||"), ("written", "move ||")] {
        let source = with_owners(&format!(
            r#"
func main() -> i32 {{
    val s = "a" + "b"
    val outer = || -> u64 {{
        val inner = {inner} -> u64 {{ consume(s) }}
        inner()
    }}
    return outer() as i32
}}
"#
        ));
        assert_eq!(run(&format!("nested_{name}.nr"), &source), 2);
    }
}

/// A default body reaches every impl, so the literal lowering reads must be one the
/// checker recorded on.
#[test]
fn a_moved_capture_reaches_lowering_from_a_default_method() {
    let source = with_owners(
        r#"
trait Named {
    func name(&self) -> string
    func measure(&self) -> u64 {
        val s = self.name()
        val f = || -> u64 { consume(s) }
        f()
    }
}
struct A { }
impl Named for A {
    func name(&self) -> string { "abc" + "d" }
}
func main() -> i32 {
    val a = A { }
    return a.measure() as i32
}
"#,
    );
    assert_eq!(run("inferred_default.nr", &source), 4);
}

#[test]
fn a_move_out_of_a_capture_in_a_loop_is_refused() {
    let err = rejected(
        "inferred_loop.nr",
        &with_owners(
            r#"
func main() -> i32 {
    val s = "a" + "b"
    for i in 0..2 {
        val f = || -> u64 { consume(s) }
        f()
    }
    return 0
}
"#,
        ),
    );
    assert!(err.contains("inside a loop body"), "{err}");
}

/// A built-in runs its function per element, so a `once` one would release the capture
/// again on every element past the first.
#[test]
fn a_once_closure_is_not_handed_to_a_per_element_built_in() {
    for head in ["|x: i32| -> i32", "move |x: i32| -> i32"] {
        let err = rejected(
            "once_per_element.nr",
            &with_owners(&format!(
                r#"
func main() -> i32 {{
    val n = Noisy {{ id: 1 }}
    val xs = [1, 2, 3]
    for y in xs.map({head} {{ eat(n) + x }}) {{ }}
    return 0
}}
"#
            )),
        );
        assert!(err.contains("once per element"), "{err}");
    }
}

#[test]
fn a_moved_capture_is_gone_from_its_source() {
    let err = rejected(
        "move_then_use.nr",
        r#"
func main() -> i32 {
    val s = "a" + "b"
    val f = move || -> u64 { s.len() }
    println(s)
    return f() as i32
}
"#,
    );
    assert!(err.contains("use of moved value 's'"), "{err}");
}

#[test]
fn a_borrowed_capture_freezes_its_binding_while_the_closure_lives() {
    let moved = rejected(
        "move_while_borrowed.nr",
        r#"
func consume(s: string) -> u64 { s.len() }
func main() -> i32 {
    val s = "a" + "b"
    val f = || -> u64 { s.len() }
    val n = consume(s)
    return f() as i32
}
"#,
    );
    assert!(
        moved.contains("cannot move out of 's' while it is borrowed"),
        "{moved}"
    );

    let read = rejected(
        "read_while_mutably_borrowed.nr",
        r#"
func main() -> i32 {
    mut c = 0
    val inc = || -> void { c += 1 }
    inc()
    return c
}
"#,
    );
    assert!(
        read.contains("cannot use 'c' while it is mutably borrowed"),
        "{read}"
    );
}

#[test]
fn a_closure_holding_a_borrow_cannot_be_stored_past_its_scope() {
    let err = rejected(
        "scope_bound_assignment.nr",
        r#"
func main() -> i32 {
    mut f = |x: i32| -> u64 { 0 }
    {
        val s = "a" + "b"
        f = |x: i32| -> u64 { s.len() }
    }
    return f(1) as i32
}
"#,
    );
    assert!(err.contains("does not live long enough"), "{err}");
}

#[test]
fn a_reading_closure_literal_is_handed_straight_to_a_built_in_method() {
    // Each literal borrows `cfg`, and once had to be bound to a `val` first.
    let source = r#"
struct Cfg { k: i32, name: string }

func main() -> i32 {
    val cfg = Cfg { k: 3, name: "a" + "bc" }
    val arr = [1, 2, 3, 4]
    mut total = 0
    for x in arr.map(|v| v * cfg.k).filter(|v| v > cfg.name.len() as i32) {
        total += x
    }
    val t: Tensor<i32, [3]> = [1, 2, 3]
    val u = t.map(|e| e + cfg.name.len() as i32)
    val s = t.zip(&u, |a, b| a * b + cfg.k)
    val r = t.reduce(0, |acc, e| acc + e * cfg.k)
    return total + u[2] + s[0] + r
}
"#;
    // 6 + 9 + 12, then 6, then 1 * 4 + 3, then 3 * (1 + 2 + 3).
    assert_eq!(run("builtin_literal.nr", source), 27 + 6 + 7 + 18);
}

#[test]
fn a_for_head_literal_holds_its_borrows_until_the_loop_ends() {
    let err = rejected(
        "adapter_borrow_held.nr",
        r#"
func main() -> i32 {
    val arr = [1, 2, 3, 4]
    mut lim = "a" + "b"
    mut total = 0
    for x in arr.filter(|e| e > lim.len() as i32) {
        lim = "x" + "yzw"
        total += x
    }
    lim = "q" + "q"
    return total
}
"#,
    );
    assert!(
        err.contains("cannot assign to 'lim' while it is borrowed"),
        "{err}"
    );
    assert_eq!(err.matches("cannot assign to 'lim'").count(), 1, "{err}");
}

#[test]
fn a_mutating_closure_literal_is_not_handed_to_a_built_in_method() {
    let err = rejected(
        "builtin_mutating_literal.nr",
        r#"
func main() -> i32 {
    mut v: Vec<i32> = Vec::new()
    v.push(1)
    mut total = 0
    for x in v.filter(|e| {
        v.push(5)
        true
    }) {
        total += x
    }
    val t: Tensor<i32, [3]> = [1, 2, 3]
    mut c = "a" + "b"
    val u = t.map(|e| {
        c = "x" + "y"
        e
    })
    return total
}
"#,
    );
    assert_eq!(
        err.matches("a closure literal that writes through a borrow")
            .count(),
        2,
        "{err}"
    );
}
