// End-to-end tests for the release of anonymous heap `string`s: the owned buffer a
// `+`, an interpolation, or `StringBuilder::to_string` allocates in an expression that binds
// it to nothing.
//
// A leak has no exit code, so each program below runs its leaking shape a few hundred
// thousand times over a payload wide enough that the unreleased buffers add up to tens
// of megabytes: without the release the process's heap climbs, with it it stays flat.
// What is asserted is the arithmetic and that the run completes, the same way
// `string_builder.rs` catches a builder that never frees its buffer; the count of
// releases the backend actually emits is asserted directly in `llvm-backend`'s own
// tests, which is where a regression shows up as a failure rather than as memory use.
use crate::compile_harness::CompileTest;

/// Rounds and payload width chosen together so one unreleased buffer per iteration is
/// tens of megabytes: enough that a regression is a visible memory event, cheap enough
/// that the loop itself costs a fraction of a second.
const LEAK_ROUNDS: u32 = 200_000;

/// The concatenation payload, 64 bytes, repeated as both operands throughout.
const PAYLOAD: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// `PAYLOAD.len()`, as the byte arithmetic each program checks itself against.
const WIDTH: u64 = PAYLOAD.len() as u64;

#[test]
fn a_concatenation_chain_does_not_leak_its_intermediate_results() {
    let test = CompileTest::new();
    let source = format!(
        r#"
func main() -> i32 {{
    val a = "{PAYLOAD}"
    mut i: u32 = 0
    mut n: u64 = 0
    while i < {LEAK_ROUNDS} {{
        val s = a + a + a
        n = n + s.len()
        i = i + 1
    }}
    if n != {} {{
        return 91
    }}
    0
}}
"#,
        u64::from(LEAK_ROUNDS) * WIDTH * 3
    );
    let exit = test
        .compile_and_run("concat_chain.nr", &source)
        .expect("compile/run failed");
    assert_eq!(exit, 0);
}

#[test]
fn an_operand_built_for_a_comparison_is_released_at_it() {
    let test = CompileTest::new();
    let source = format!(
        r#"
func main() -> i32 {{
    val a = "{PAYLOAD}"
    mut i: u32 = 0
    mut hits: u32 = 0
    while i < {LEAK_ROUNDS} {{
        if a + a == "{PAYLOAD}{PAYLOAD}" {{
            hits = hits + 1
        }}
        i = i + 1
    }}
    if hits != {LEAK_ROUNDS} {{
        return 91
    }}
    0
}}
"#
    );
    let exit = test
        .compile_and_run("compare_operand.nr", &source)
        .expect("compile/run failed");
    assert_eq!(exit, 0);
}

#[test]
fn a_receiver_and_a_push_str_argument_built_for_their_call_are_released_at_it() {
    let test = CompileTest::new();
    let source = format!(
        r#"
func main() -> i32 {{
    val a = "{PAYLOAD}"
    mut i: u32 = 0
    mut n: u64 = 0
    while i < {LEAK_ROUNDS} {{
        n = n + (a + a).len()
        mut b = StringBuilder::new()
        b.push_str(a + a)
        n = n + b.len()
        i = i + 1
    }}
    if n != {} {{
        return 91
    }}
    0
}}
"#,
        u64::from(LEAK_ROUNDS) * WIDTH * 4
    );
    let exit = test
        .compile_and_run("call_temporaries.nr", &source)
        .expect("compile/run failed");
    assert_eq!(exit, 0);
}

#[test]
fn a_statement_whose_value_nothing_reads_releases_it() {
    let test = CompileTest::new();
    let source = format!(
        r#"
func main() -> i32 {{
    val a = "{PAYLOAD}"
    mut i: u32 = 0
    while i < {LEAK_ROUNDS} {{
        a + a
        "{{a}} and {{a}}"
        i = i + 1
    }}
    0
}}
"#
    );
    let exit = test
        .compile_and_run("discarded_value.nr", &source)
        .expect("compile/run failed");
    assert_eq!(exit, 0);
}

/// `StringBuilder::to_string` allocates on every call, so the `string` it hands back is owned
/// by whatever takes it: a binding releases it at scope exit, and a consumer that
/// discards it releases it there.
#[test]
fn the_builder_copy_out_is_owned_by_whatever_takes_it() {
    let test = CompileTest::new();
    let source = format!(
        r#"
func main() -> i32 {{
    mut i: u32 = 0
    mut n: u64 = 0
    while i < {LEAK_ROUNDS} {{
        mut b = StringBuilder::new()
        b.push_str("{PAYLOAD}")
        val s = b.to_string()
        n = n + s.len()
        n = n + b.to_string().len()
        i = i + 1
    }}
    if n != {} {{
        return 91
    }}
    0
}}
"#,
        u64::from(LEAK_ROUNDS) * WIDTH * 2
    );
    let exit = test
        .compile_and_run("builder_copy_out.nr", &source)
        .expect("compile/run failed");
    assert_eq!(exit, 0);
}

/// The release is a release, not a `free`: a concatenation inside a `pool` block draws
/// its buffer from the arena, and handing an arena pointer to `free` would abort. The
/// arena's own sweep at the closing brace is what reclaims it.
#[test]
fn a_temporary_allocated_in_a_pool_is_left_to_the_arena() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a = "0123456789abcdef"
    mut n: u64 = 0
    pool {
        mut i: u32 = 0
        while i < 1000 {
            n = n + (a + a + a).len()
            i = i + 1
        }
    }
    if n != 48000 {
        return 91
    }
    0
}
"#;
    let exit = test
        .compile_and_run("pooled_temporary.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 0);
}

/// A buffer a binding still owns is not released by a consumer that merely reads it:
/// `s.len()` and `s == t` take a place expression, which `produces_owned_string` answers
/// `false` for, so the binding's own scope-exit release stays the only one.
#[test]
fn a_bound_buffer_is_not_released_twice_by_a_reader() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a = "abc"
    val s = a + "def"
    val t = a + "def"
    if s.len() != 6 {
        return 91
    }
    if s != t {
        return 92
    }
    mut b = StringBuilder::new()
    b.push_str(s)
    b.push_str(t)
    b.len() as i32
}
"#;
    let exit = test
        .compile_and_run("bound_not_double_released.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 12);
}

/// The holder rule, over a loop: a struct field takes ownership of the buffer stored
/// into it, and the holder's destruction at the end of each iteration releases it. Held
/// by nobody, this is one leaked buffer per round.
#[test]
fn a_struct_field_releases_the_buffer_it_was_given() {
    let test = CompileTest::new();
    let source = format!(
        r#"
struct Line {{ text: string, n: u64 }}

func main() -> i32 {{
    val a = "{PAYLOAD}"
    mut i: u32 = 0
    mut n: u64 = 0
    while i < {LEAK_ROUNDS} {{
        val line = Line {{ text: a + a, n: 2 }}
        n = n + line.text.len()
        i = i + 1
    }}
    if n != {} {{
        return 91
    }}
    0
}}
"#,
        u64::from(LEAK_ROUNDS) * WIDTH * 2
    );
    let exit = test
        .compile_and_run("field_release.nr", &source)
        .expect("compile/run failed");
    assert_eq!(exit, 0);
}

/// A field that is reassigned releases what it displaces, and takes on the replacement
/// only when the replacement allocated. The literal round must leave the field pointing
/// at `.rodata` with nothing armed, or the next round frees `.rodata`.
#[test]
fn a_reassigned_field_releases_the_buffer_it_displaces() {
    let test = CompileTest::new();
    let source = format!(
        r#"
struct Line {{ text: string }}

func main() -> i32 {{
    val a = "{PAYLOAD}"
    mut line = Line {{ text: a + a }}
    mut i: u32 = 0
    mut n: u64 = 0
    while i < {LEAK_ROUNDS} {{
        line.text = a + a
        n = n + line.text.len()
        line.text = "short"
        n = n + line.text.len()
        i = i + 1
    }}
    if n != {} {{
        return 91
    }}
    0
}}
"#,
        u64::from(LEAK_ROUNDS) * (WIDTH * 2 + 5)
    );
    let exit = test
        .compile_and_run("field_reassign_release.nr", &source)
        .expect("compile/run failed");
    assert_eq!(exit, 0);
}

/// A function that allocates on every return path hands the buffer to its caller, which
/// is the only place that can release it. The literal-returning sibling in the same
/// program must stay borrowed: releasing its result would hand `.rodata` to `free`.
#[test]
fn a_returned_buffer_is_released_by_its_caller() {
    let test = CompileTest::new();
    let source = format!(
        r#"
func doubled(s: &string) -> string {{
    return s + s
}}

func fixed() -> string {{
    return "borrowed"
}}

func main() -> i32 {{
    val a = "{PAYLOAD}"
    mut i: u32 = 0
    mut n: u64 = 0
    while i < {LEAK_ROUNDS} {{
        val built = doubled(&a)
        n = n + built.len() + fixed().len()
        i = i + 1
    }}
    if n != {} {{
        return 91
    }}
    0
}}
"#,
        u64::from(LEAK_ROUNDS) * (WIDTH * 2 + 8)
    );
    let exit = test
        .compile_and_run("returned_release.nr", &source)
        .expect("compile/run failed");
    assert_eq!(exit, 0);
}

/// A buffer handed to a parameter the callee only reads is dead the moment the call
/// returns, so the caller releases it there.
#[test]
fn an_argument_a_callee_only_reads_is_released_at_the_call() {
    let test = CompileTest::new();
    let source = format!(
        r#"
func width(s: string) -> u64 {{
    return s.len()
}}

func main() -> i32 {{
    val a = "{PAYLOAD}"
    mut i: u32 = 0
    mut n: u64 = 0
    while i < {LEAK_ROUNDS} {{
        n = n + width(a + a)
        i = i + 1
    }}
    if n != {} {{
        return 91
    }}
    0
}}
"#,
        u64::from(LEAK_ROUNDS) * WIDTH * 2
    );
    let exit = test
        .compile_and_run("argument_release.nr", &source)
        .expect("compile/run failed");
    assert_eq!(exit, 0);
}

// BUG-057: `string.clone()` is a deep copy. It handed back the receiver's own fat
// pointer, so a clone returned out of the function that owned the original pointed at
// a buffer freed on the way out, and the filler loop below reused it.
#[test]
fn test_bug_057_a_string_clone_outlives_its_original() {
    let test = CompileTest::new();
    let source = r#"
func make() -> string {
    val a = "hello-" + "world"
    a.clone()
}

func main() -> i32 {
    val s = make()
    mut i = 0
    while i < 50 {
        val filler = "XXXXXXXXXXXX-{i}"
        i += 1
    }
    if s == "hello-world" {
        return 0
    }
    return 1
}
"#;
    let exit = test
        .compile_and_run("bug_057_string_clone.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 0);
}

/// Regression test for BUG-059: a `return` written inside an expression (a `loop` body, an
/// `if` used as a binding's value, a `match` arm) is an exit of the function like any other. The summary
/// of functions that return an owned buffer did not see those exits, so a function that
/// could return a literal through one was read as allocating on every path, and its caller
/// handed the literal's `.rodata` to `free`.
#[test]
fn test_bug_059_a_return_inside_an_expression_is_an_exit() {
    let test = CompileTest::new();
    let source = r#"
func looped(c: bool, a: &string) -> string {
    loop {
        if c { return "lit" }
        break
    }
    a + "?"
}

func bound(c: bool, a: &string) -> string {
    val x = if c { return "lit" } else { 1 }
    a + "{x}"
}

func matched(n: i32, a: &string) -> string {
    val k = match n {
        0 => return "lit",
        _ => 1
    }
    a + "{k}"
}

func main() -> i32 {
    val a = "a"
    val s = looped(true, &a)
    val t = bound(true, &a)
    val u = looped(false, &a)
    val v = matched(0, &a)
    return (s.len() + t.len() + u.len() + v.len()) as i32
}
"#;
    let exit = test
        .compile_and_run("return_inside_expression.nr", source)
        .expect("compile/run failed");
    // 3 + 3 + 2 + 3; freeing a literal aborts instead.
    assert_eq!(exit, 11);
}

/// A function that hands back its own parameter gives its caller the buffer the caller
/// passed, owned exactly when the argument was. Each round forwards an owned buffer, a
/// literal (which must never reach `free`), a buffer through two forwarders, and one
/// through a `mut` reassignment. A forward read as allocating frees the literal and
/// aborts; one read as owned twice double-frees; one read as owning nothing leaks a
/// buffer per round, which is what every forward did.
#[test]
fn test_bug_039_a_forwarded_parameter_is_released_once_by_the_caller() {
    let test = CompileTest::new();
    let source = format!(
        r#"
func keep(s: string) -> string {{ s }}
func relay(s: string) -> string {{ return keep(s) }}

func main() -> i32 {{
    val a = "{PAYLOAD}"
    mut i: u32 = 0
    mut n: u64 = 0
    mut last = "start"
    while i < {LEAK_ROUNDS} {{
        val owned = keep(a + a)
        val literal = keep("fixed")
        val bound = a + a
        val twice = relay(bound)
        val again = a + a
        last = keep(again)
        n = n + owned.len() + literal.len() + twice.len() + last.len()
        i = i + 1
    }}
    if n != {} {{
        return 91
    }}
    0
}}
"#,
        u64::from(LEAK_ROUNDS) * (WIDTH * 6 + 5)
    );
    let exit = test
        .compile_and_run("forwarded_release.nr", &source)
        .expect("compile/run failed");
    assert_eq!(exit, 0);
}

/// A store through a borrow cannot see the flag its place's owner keeps, and that flag,
/// armed or not, is what releases the position at the owner's scope exit. A literal
/// stored through a `&mut string`, a `&mut` struct, a `&mut self` receiver or a whole
/// `*n = Named { .. }` was freed as the heap buffer the owner's armed flag promised, and
/// the program aborted in `free`. Every position such a store writes now owns heap bytes.
#[test]
fn test_bug_089_a_literal_stored_through_a_borrow_is_never_freed_as_a_buffer() {
    let test = CompileTest::new();
    let source = format!(
        r#"
struct Named {{ name: string }}

impl Named {{
    func set(&mut self) {{ self.name = "method" }}
}}

func relabel(s: &mut string) {{ *s = "literal" }}
func rename(n: &mut Named) {{ n.name = "field" }}
func swap(n: &mut Named) {{ *n = Named {{ name: "whole" }} }}
func slot(names: &mut [string; 2]) {{ names[1] = "element" }}
func build(s: &mut string) {{ *s = "built" + "!" }}

func main() -> i32 {{
    val a = "{PAYLOAD}"
    mut i: u32 = 0
    mut n: u64 = 0
    while i < {LEAK_ROUNDS} {{
        mut s = a + a
        relabel(&mut s)
        mut b = a + a
        build(&mut b)
        mut r = Named {{ name: a + a }}
        rename(&mut r)
        mut w = Named {{ name: a + a }}
        swap(&mut w)
        mut m = Named {{ name: a + a }}
        m.set()
        mut names = [a + a, a + a]
        slot(&mut names)
        n = n + s.len() + b.len() + r.name.len() + w.name.len() + m.name.len() + names[1].len()
        i = i + 1
    }}
    if n != {} {{
        return 91
    }}
    0
}}
"#,
        u64::from(LEAK_ROUNDS) * (7 + 6 + 5 + 5 + 6 + 7)
    );
    let exit = test
        .compile_and_run("stored_through_borrow.nr", &source)
        .expect("compile/run failed");
    assert_eq!(exit, 0);
}

#[test]
fn regression_bug_103_a_string_chosen_by_a_branch_is_released_once() {
    // Each `string` below is owned by the binding that takes it and released once:
    // a function with a literal arm beside an allocating one (the literal is copied), a
    // binding of an `if` or `match` whose every arm allocates, a block's value, a
    // reordered named call, and a method. A literal-only branch is never freed. The
    // leaks are LeakSanitizer's to see; what is asserted here is the arithmetic and a
    // clean exit, which a double free would not give.
    let exit = CompileTest::new()
        .compile_and_run(
            "owned_branches.nr",
            r#"
struct Tag { k: i32 }
impl Tag {
    func name(&self) -> string { if self.k == 0 { "zero" } else { "tag {self.k}" } }
}
func label(k: i32) -> string {
    match k {
        0 => "info",
        _ => "warn {k}",
    }
}
func pick(flag: bool, a: string, b: string) -> string {
    if flag { return "none" }
    return a + b
}
func lits(k: i32) -> string { if k == 0 { "a" } else { "bb" } }
func cat(first: string, second: string) -> string { return "{first}{second}" }
func s(v: string) -> string { return v }
func main() -> i32 {
    mut t: u64 = 0
    for i in 0..3 {
        val a = label(i)
        val b = if i == 0 { "z {i}" } else { "n {i}" }
        val c = match i { 0 => "x", 1 => "yy", _ => "zzz" }
        val d = lits(i)
        val e = pick(i == 1, "p", "q{i}")
        val s1: string = "x{i}"
        val f: string = {
            val u = s1
            cat(u, "y")
        }
        val g = Tag { k: i }.name()
        val h = cat(second: "w{i}", first: "v")
        t = t + a.len() + b.len() + c.len() + d.len() + e.len() + f.len() + g.len() + h.len()
    }
    t as i32
}
"#,
        )
        .expect("should compile and run");
    assert_eq!(exit, 78);
}

#[test]
fn regression_bug_103_an_owned_branch_value_moves_on_and_a_returned_val_is_owned() {
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "owned_branch_moves.nr",
            r#"
func keep(s: string) -> string { s }
func size(s: string) -> u64 { s.len() }
func build(c: bool, i: i32) -> string {
    val b = if c { "a{i}" } else { "bb{i}" }
    b
}
func main() -> i32 {
    mut v: Vec<string> = Vec::new()
    mut t: u64 = 0
    for i in 0..2 {
        val b = if i == 0 { "x{i}" } else { "yy{i}" }
        val k = keep(b)
        t = t + k.len()
        val m = match i { 0 => "m{i}", _ => "mm{i}" }
        t = t + size(m)
        v.push(if i == 0 { "p{i}" } else { "q{i}" })
        mut r = if i == 1 { "r{i}" } else { "rr{i}" }
        r = build(i == 0, i)
        t = t + r.len()
    }
    t = t + v[0].len() + v[1].len()
    t as i32 + 16
}
"#,
        )
        .expect("should compile and run");
    assert_eq!(exit, 35);
    let exit = test
        .compile_and_run(
            "return_binding.nr",
            r#"
func build(i: i32) -> string {
    val b = "ab{i}"
    b
}
func main() -> i32 {
    mut t: u64 = 0
    for i in 0..3 { t = t + build(i).len() }
    t as i32
}
"#,
        )
        .expect("should compile and run");
    assert_eq!(exit, 9);
}

#[test]
fn regression_bug_107_a_forwarded_string_passed_to_a_reading_parameter_is_released() {
    // `s` hands back the `string` it was given. Its result, passed straight on to a
    // parameter that only reads it, is released after the call when the buffer was
    // owned, and left alone when it was a literal. Functions and methods alike.
    let exit = CompileTest::new()
        .compile_and_run(
            "forwarded_args.nr",
            r#"
struct Box1 { n: i32 }
impl Box1 {
    func pass(&self, v: string) -> string { v }
    func size(&self, v: string) -> u64 { v.len() }
}
func s(v: string) -> string {
    return v
}
func cat(first: string, second: string) -> string { return "{first}{second}" }
func size(v: string) -> u64 { v.len() }
func main() -> i32 {
    val b = Box1 { n: 1 }
    mut t: u64 = 0
    for i in 0..3 {
        val s1: string = "x{i}"
        val owned = "o{i}"
        val lit = "lit"
        val a = cat(s("y{i}"), s1)
        val c = cat(s("k"), s(owned))
        val d = size(s(lit)) + size(s("q{i}"))
        val e = b.size(b.pass("m{i}")) + b.size(s("r"))
        t = t + a.len() + c.len() + d + e
    }
    t as i32
}
"#,
        )
        .expect("should compile and run");
    assert_eq!(exit, 45);
}

#[test]
fn regression_bug_038_a_closure_call_releases_what_it_reads_and_returns() {
    // A closure called through its binding, in place, or through `|>` that only reads
    // its argument leaves it to the caller, an owned result belongs to whoever takes it,
    // and a forwarded one is owned when its argument was. Without these releases the loop
    // leaks a buffer per call.
    let test = CompileTest::new();
    let source = format!(
        r#"
func main() -> i32 {{
    val width = |z: string| -> u64 {{ z.len() }}
    val twice = |z: string| -> string {{ z + z }}
    val keep = |z: string| -> string {{ z }}
    val a = "{PAYLOAD}"
    mut i: u32 = 0
    mut n: u64 = 0
    while i < {LEAK_ROUNDS} {{
        val s = a + a
        n = n + width(s)
        n = n + (a + a |> width)
        n = n + (|z: string| -> u64 {{ z.len() }})(a + a)
        val t = twice(a.clone())
        n = n + t.len()
        val k = keep(a + a)
        n = n + k.len()
        i = i + 1
    }}
    if n != {} {{
        return 91
    }}
    0
}}
"#,
        u64::from(LEAK_ROUNDS) * WIDTH * 10
    );
    let exit = test
        .compile_and_run("closure_release.nr", &source)
        .expect("compile/run failed");
    assert_eq!(exit, 0);
}
