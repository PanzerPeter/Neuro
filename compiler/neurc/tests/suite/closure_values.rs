//! Borrowing closures as values: a closure that borrows holds its borrows the way a struct
//! holding a reference does, so it may be stored, assigned and returned wherever what it
//! borrows lives long enough, and a combinator may return a closure over its function
//! parameters, whose result holds what its arguments borrowed.

use crate::compile_harness::{CompileTest, rejected, stdout_of};

fn leak_checked(name: &str, source: &str) -> i32 {
    CompileTest::new().run_leak_checked(name, source)
}

const COMPOSE: &str =
    "func compose(f: (i32) -> i32, g: (i32) -> i32) -> (i32) -> i32 { |x| g(f(x)) }\n";

#[test]
fn a_borrowing_closure_sits_in_a_struct_a_tuple_an_array_and_a_payload() {
    let exit = leak_checked(
        "stored.nr",
        r#"
struct H { f: (i32) -> i32, tag: i32 }
enum E { A((i32) -> i32), B }

func main() -> i32 {
    val s = "ab" + "cd"
    val h = H { f: |x| x + s.len() as i32, tag: 1 }
    val t = (|x: i32| x * s.len() as i32, 2)
    val a = [|x: i32| x - s.len() as i32, |x: i32| x]
    val e = E::A(|x| x + 100 + s.len() as i32)
    val o = Some(|x: i32| x + 1000 + s.len() as i32)
    val from_payload = match e {
        E::A(f) => f(0),
        E::B => 0,
    }
    mut from_option = 0
    if val Some(f) = o {
        from_option = f(0)
    }
    (h.f)(1) + (t.0)(2) + (a[0])(5) + from_payload + from_option - 1100
}
"#,
    );
    assert_eq!(exit, 22); // 5 + 8 + 1 + 104 + 1004 - 1100
}

#[test]
fn a_borrowing_closure_is_assigned_to_a_binding_its_borrowee_outlives() {
    let exit = leak_checked(
        "assigned.nr",
        r#"
struct H { f: (i32) -> i32 }

func main() -> i32 {
    val s = "abc" + "d"
    mut f: (i32) -> i32 = |x| x
    mut h = H { f: |x| x }
    {
        f = |x| x + s.len() as i32
        h.f = |x| x * s.len() as i32
    }
    f(1) + (h.f)(2)
}
"#,
    );
    assert_eq!(exit, 13);
}

#[test]
fn an_exclusive_borrow_rides_in_a_struct() {
    let exit = leak_checked(
        "exclusive.nr",
        r#"
struct C { f: mut () -> i32 }

func main() -> i32 {
    mut n = 0
    val c = C { f: || {
        n += 1
        n
    } }
    (c.f)() + (c.f)()
}
"#,
    );
    assert_eq!(exit, 3);
}

#[test]
fn a_combinator_returns_a_closure_over_its_function_parameters() {
    let exit = leak_checked(
        "compose.nr",
        &format!(
            r#"{COMPOSE}
func compose_all<A, B, C>(f: (A) -> B, g: (B) -> C) -> (A) -> C {{ |x| g(f(x)) }}
func pass(f: (i32) -> i32) -> (i32) -> i32 {{ f }}

func main() -> i32 {{
    val s = "ab" + "c"
    val add = |x: i32| x + s.len() as i32
    val c = compose(add, |x| x * 2)
    val twice = compose(c, pass(c))
    val generic = compose_all(add, |y: i32| y + 1)
    twice(0) + generic(0)
}}
"#
        ),
    );
    assert_eq!(exit, 22); // ((0 + 3) * 2 + 3) * 2 + 4
}

#[test]
fn a_once_parameter_leaves_the_call() {
    let exit = leak_checked(
        "once.nr",
        r#"
func keep(f: once () -> i32) -> once () -> i32 { move || f() + 1 }

func main() -> i32 {
    val s = "abc" + "d"
    val g = keep(move || s.len() as i32)
    g()
}
"#,
    );
    assert_eq!(exit, 5);
}

#[test]
fn a_function_value_stored_through_a_call_keeps_its_bytes() {
    let out = stdout_of(
        "store_call.nr",
        r#"
struct H { f: () -> void }
func set(h: &mut H, f: () -> void) { h.f = f }

func main() -> i32 {
    val s = "bor" + "rowed"
    mut h = H { f: || println("none") }
    set(&mut h, || println(s))
    (h.f)()
    0
}
"#,
    );
    assert_eq!(out, "borrowed\n");
}

#[test]
fn a_reference_argument_stays_free_of_a_holder_of_function_values() {
    let exit = leak_checked(
        "step.nr",
        r#"
struct Opt { lr: (i32) -> i32, total: i32 }

impl Opt {
    func step(&mut self, batch: &string) {
        self.total += (self.lr)(batch.len() as i32)
    }
}

func main() -> i32 {
    mut o = Opt { lr: |x| x * 2, total: 0 }
    for i in 0..3 {
        val batch = "ab" + "c"
        o.step(&batch)
    }
    o.total
}
"#,
    );
    assert_eq!(exit, 18);
}

/// Each refusal the borrowee's lifetime calls for, wherever the closure went.
#[test]
fn a_held_borrow_freezes_its_borrowee() {
    for (name, held) in [
        ("in_struct.nr", "H { f: || s.len() }"),
        ("in_tuple.nr", "(|| s.len(), 1)"),
        ("in_array.nr", "[|| s.len(), || s.len()]"),
        ("in_option.nr", "Some(|| s.len())"),
        ("through_call.nr", "pass(|| s.len())"),
        ("through_generic.nr", "same(|| s.len())"),
    ] {
        let err = rejected(
            name,
            &format!(
                r#"struct H {{ f: () -> u64 }}
func pass(f: () -> u64) -> () -> u64 {{ f }}
func same<T>(x: T) -> T {{ x }}
func main() -> i32 {{
    val s = "a" + "b"
    val held = {held}
    val moved = s
    0
}}
"#
            ),
        );
        assert!(err.contains("cannot move out of 's'"), "{name}: {err}");
    }
}

#[test]
fn a_closure_cannot_be_stored_in_a_binding_that_outlives_its_borrowee() {
    for (name, store) in [
        ("whole.nr", "f = || s.len()"),
        ("field.nr", "h.f = || s.len()"),
        ("through_call.nr", "set(&mut h, || s.len())"),
        ("generic_call.nr", "put(&mut slot, || s.len())"),
    ] {
        let err = rejected(
            name,
            &format!(
                r#"struct H {{ f: () -> u64 }}
struct Slot<T> {{ v: T }}
func set(h: &mut H, f: () -> u64) {{ h.f = f }}
func put<T>(h: &mut Slot<T>, v: T) {{ h.v = v }}
func main() -> i32 {{
    val zero: () -> u64 = || 0
    mut f = zero
    mut h = H {{ f: zero }}
    mut slot = Slot {{ v: zero }}
    {{
        val s = "a" + "b"
        {store}
    }}
    0
}}
"#
            ),
        );
        assert!(err.contains("does not live long enough"), "{name}: {err}");
    }
}

#[test]
fn a_closure_over_a_local_cannot_leave_the_function() {
    for (name, item) in [
        (
            "literal.nr",
            "func f() -> (i32) -> u64 {\n    val s = \"a\" + \"b\"\n    |x| s.len()\n}",
        ),
        (
            "binding.nr",
            "func f() -> (i32) -> u64 {\n    val s = \"a\" + \"b\"\n    val c = |x: i32| s.len()\n    return c\n}",
        ),
        (
            "parameter.nr",
            "func f(s: string) -> (i32) -> u64 { |x| s.len() }",
        ),
        (
            "struct.nr",
            "struct H { f: (i32) -> u64 }\nfunc f() -> H {\n    val s = \"a\" + \"b\"\n    H { f: |x| s.len() }\n}",
        ),
        (
            "through_call.nr",
            "func pass(f: (i32) -> u64) -> (i32) -> u64 { f }\nfunc f() -> (i32) -> u64 {\n    val s = \"a\" + \"b\"\n    pass(|x| s.len())\n}",
        ),
        (
            "copied.nr",
            "func f() -> (i32) -> u64 {\n    val s = \"a\" + \"b\"\n    val c = |x: i32| s.len()\n    move |x| c(x)\n}",
        ),
    ] {
        let err = rejected(name, &format!("{item}\nfunc main() -> i32 {{ 0 }}\n"));
        assert!(
            err.contains("cannot return a reference to 's'"),
            "{name}: {err}"
        );
    }
}

/// A closure's environment points at the slot of what it borrows, and a reference
/// parameter's or `self`'s slot dies with the call even though its referent does not.
#[test]
fn a_closure_over_a_parameter_slot_cannot_leave_the_function() {
    for (name, item, slot) in [
        (
            "mut_reference.nr",
            "func f(r: &mut i32) -> mut () -> i32 {\n    val c = || {\n        *r += 1\n        *r\n    }\n    c\n}",
            "r",
        ),
        (
            "self_literal.nr",
            "struct S { n: string }\nimpl S { func f(&self) -> () -> u64 { || self.n.len() } }",
            "self",
        ),
        (
            "self_copied.nr",
            "struct S { n: string }\nimpl S {\n    func f(&self) -> () -> u64 {\n        val c = || self.n.len()\n        val d = c\n        d\n    }\n}",
            "self",
        ),
    ] {
        let err = rejected(name, &format!("{item}\nfunc main() -> i32 {{ 0 }}\n"));
        assert!(
            err.contains(&format!("reference to '{slot}'")),
            "{name}: {err}"
        );
    }
}

#[test]
fn a_mut_function_parameter_cannot_leave_the_call() {
    for (name, item) in [
        (
            "returned.nr",
            "func f(g: mut () -> i32) -> mut () -> i32 { g }",
        ),
        (
            "closed_over.nr",
            "func f(g: mut () -> i32) -> mut () -> i32 { || g() }",
        ),
        (
            "moved_in.nr",
            "func f(g: mut () -> i32) -> mut () -> i32 { move || g() }",
        ),
        (
            "stored.nr",
            "struct C { f: mut () -> i32 }\nfunc f(g: mut () -> i32) -> C { C { f: g } }",
        ),
    ] {
        let err = rejected(name, &format!("{item}\nfunc main() -> i32 {{ 0 }}\n"));
        assert!(err.contains("stays in the call"), "{name}: {err}");
    }
}

/// A value leaving a block, an arm or a closure body may not hold a borrow of what that
/// scope declares. References had this hole too: a reference, a struct around one and a
/// method's result compiled and read freed memory, because the borrow was attached after
/// the binding's scope had closed.
#[test]
fn regression_a_scope_may_not_yield_a_borrow_of_its_own_binding() {
    for (name, value) in [
        (
            "reference.nr",
            "{\n        val s = \"a\" + \"b\"\n        &s\n    }",
        ),
        (
            "struct.nr",
            "{\n        val s = \"a\" + \"b\"\n        R { r: &s }\n    }",
        ),
        (
            "closure.nr",
            "{\n        val s = \"a\" + \"b\"\n        |x: i32| s.len()\n    }",
        ),
        (
            "closure_binding.nr",
            "{\n        val s = \"a\" + \"b\"\n        val c = |x: i32| s.len()\n        H { f: c }\n    }",
        ),
        (
            "if_arm.nr",
            "if 1 > 0 {\n        val s = \"a\" + \"b\"\n        |x: i32| s.len()\n    } else {\n        |x: i32| 0\n    }",
        ),
        (
            "match_binding.nr",
            "match Some(\"a\" + \"b\") {\n        Some(s) => |x: i32| s.len(),\n        None => |x: i32| 0,\n    }",
        ),
        (
            "method_result.nr",
            "{\n        val s = W { n: \"a\" + \"b\" }\n        s.name()\n    }",
        ),
        ("closure_parameter.nr", "|s: string| -> &string { &s }"),
        ("closure_result.nr", "|s: string| |x: i32| s.len()"),
    ] {
        let err = rejected(
            name,
            &format!(
                "struct R<'a> {{ r: &'a string }}\nstruct H {{ f: (i32) -> u64 }}\nstruct W {{ n: string }}\nimpl W {{ func name(&self) -> &string {{ &self.n }} }}\nfunc main() -> i32 {{\n    val v = {value}\n    0\n}}\n"
            ),
        );
        assert!(
            err.contains("'s' does not live long enough"),
            "{name}: {err}"
        );
    }
}

/// A reborrow through a reference dies with its referent, not with the reference, and a
/// method's reference comes from its receiver, so neither is a borrow of the block.
#[test]
fn a_block_may_yield_what_outlives_it() {
    let exit = leak_checked(
        "yields.nr",
        r#"
struct P { n: string }
struct W { v: i32 }
impl W {
    func pick(&self, _other: &i32) -> &i32 { &self.v }
}

func main() -> i32 {
    val p = P { n: "ab" + "c" }
    val name = {
        val r = &p
        &r.n
    }
    val w = W { v: 4 }
    val v = {
        val key = 1
        w.pick(&key)
    }
    val s = "abcd" + "e"
    val f = if name.len() > 1 { |x: i32| x + s.len() as i32 } else { |x: i32| x }
    f(*v)
}
"#,
    );
    assert_eq!(exit, 9);
}

#[test]
fn regression_a_generic_call_stores_into_its_mut_argument() {
    let err = rejected(
        "generic_store.nr",
        r#"
struct Slot<T> { v: T }
func put<T>(h: &mut Slot<T>, v: T) { h.v = v }
func main() -> i32 {
    val e = "e"
    mut h = Slot { v: &e }
    {
        val s = "a" + "b"
        put(&mut h, &s)
    }
    0
}
"#,
    );
    assert!(err.contains("does not live long enough"), "{err}");
}

#[test]
fn a_break_cannot_carry_a_function_value_that_borrows() {
    let err = rejected(
        "break.nr",
        r#"
func main() -> i32 {
    val s = "a" + "b"
    val f = loop {
        break |x: i32| s.len()
    }
    0
}
"#,
    );
    assert!(err.contains("a `break` cannot carry"), "{err}");
}
