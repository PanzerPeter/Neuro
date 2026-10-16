//! Closure capture modes: a capture is copied, borrowed `&` or `&mut`, or moved, as the
//! body uses it; the strongest use makes the closure `Fn`, `FnMut` or `FnOnce`, and a
//! function type's `mut` / `once` mode says which of them a parameter takes.

use crate::compile_harness::CompileTest;

fn run(filename: &str, source: &str) -> i32 {
    CompileTest::new()
        .compile_and_run(filename, source)
        .expect("compile/run failed")
}

fn refused(filename: &str, source: &str) -> String {
    CompileTest::new()
        .check(filename, source)
        .expect_err("the program should be refused")
}

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
    let err = refused(
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
    let err = refused(
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
    let err = refused(
        "once_twice.nr",
        r#"
func run(f: once () -> u64) -> u64 { f() + f() }
func main() -> i32 { return 0 }
"#,
    );
    assert!(err.contains("use of moved value 'f'"), "{err}");
}

#[test]
fn a_closure_without_move_cannot_give_a_capture_away() {
    let err = refused(
        "borrowed_consumed.nr",
        r#"
func consume(s: string) -> u64 { s.len() }
func main() -> i32 {
    val s = "a" + "b"
    val f = || -> u64 { consume(s) }
    return f() as i32
}
"#,
    );
    assert!(err.contains("write `move |...|`"), "{err}");
}

#[test]
fn a_moved_capture_is_gone_from_its_source() {
    let err = refused(
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
    let moved = refused(
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

    let read = refused(
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
    let err = refused(
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
    assert!(err.contains("lives only as long as that scope"), "{err}");
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
    let err = refused(
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
    let err = refused(
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
        err.matches("as a closure literal that holds a `&mut`")
            .count(),
        2,
        "{err}"
    );
}
