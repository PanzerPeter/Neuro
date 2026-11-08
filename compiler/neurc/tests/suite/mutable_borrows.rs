// Mutable borrow tests (Phase 1.7)
// Verifies `&mut T` reference types, `&mut place` borrow expressions, the `*`
// dereference operator (read and write), and the borrow rules: `&mut` requires a
// `mut` binding, `*` applies only to references, and writing through `*` requires
// a `&mut`. Covers end-to-end compile+run and the rejection diagnostics.
use crate::compile_harness::{assert_check_error, check_source, run_expecting};

#[test]
fn mutate_through_a_mutable_reference() {
    // The canonical example: a void function mutates its `&mut i32` argument
    // in place; the change is visible at the caller's `mut` binding.
    let source = r#"
func increment(n: &mut i32) {
    *n = *n + 1
}
func main() -> i32 {
    mut counter: i32 = 40
    increment(&mut counter)
    increment(&mut counter)
    return counter
}
"#;
    run_expecting(source, 42);
}

#[test]
fn deref_read_through_local_reference() {
    let source = r#"
func main() -> i32 {
    mut x: i32 = 7
    val r: &mut i32 = &mut x
    *r = 35
    val v: i32 = *r
    return v
}
"#;
    run_expecting(source, 35);
}

#[test]
fn mutating_function_with_extra_argument() {
    let source = r#"
func add_into(n: &mut i32, delta: i32) {
    *n = *n + delta
}
func main() -> i32 {
    mut total: i32 = 10
    add_into(&mut total, 5)
    add_into(&mut total, 12)
    return total
}
"#;
    run_expecting(source, 27);
}

#[test]
fn mutably_borrowing_an_immutable_binding_is_rejected() {
    // `&mut` demands a `mut` binding.
    let source = r#"
func main() -> i32 {
    val x: i32 = 5
    val r: &mut i32 = &mut x
    return 0
}
"#;
    let (success, stderr) = check_source(source);
    assert!(!success, "&mut of a val should be rejected");
    assert!(
        stderr.contains("cannot mutably borrow"),
        "expected a mutable-borrow diagnostic, got: {stderr}"
    );
}

#[test]
fn dereferencing_a_non_reference_is_rejected() {
    let source = r#"
func main() -> i32 {
    val x: i32 = 5
    val y: i32 = *x
    return 0
}
"#;
    let (success, stderr) = check_source(source);
    assert!(!success, "deref of a non-reference should be rejected");
    assert!(
        stderr.contains("cannot dereference"),
        "expected a dereference diagnostic, got: {stderr}"
    );
}

#[test]
fn writing_through_an_immutable_reference_is_rejected() {
    let source = r#"
func main() -> i32 {
    mut x: i32 = 5
    val r: &i32 = &x
    *r = 9
    return 0
}
"#;
    let (success, stderr) = check_source(source);
    assert!(!success, "writing through &i32 should be rejected");
    assert!(
        stderr.contains("immutable reference"),
        "expected an immutable-reference diagnostic, got: {stderr}"
    );
}

#[test]
fn mutable_and_immutable_reference_types_are_distinct() {
    // There is no `&T` -> `&mut T` strengthening: passing a `&i32` where a
    // `&mut i32` is expected is a type mismatch. The other direction is a shared
    // reborrow for the call (`a_mutable_reference_is_passed_where_a_shared_one_is_expected`).
    let source = r#"
func bump(r: &mut i32) -> i32 { *r }
func main() -> i32 {
    mut x: i32 = 5
    val v: i32 = bump(&x)
    return v
}
"#;
    let (success, stderr) = check_source(source);
    assert!(!success, "&i32 should not satisfy a &mut i32 parameter");
    assert!(
        stderr.contains("expected &mut i32, found &i32"),
        "expected a type-mismatch diagnostic, got: {stderr}"
    );
}

#[test]
fn deref_assignment_after_other_statements_parses() {
    // Regression: a `*r = v` statement following an expression-ending line must not
    // be glued to the previous expression as a multiplication continuation.
    let source = r#"
func main() -> i32 {
    mut x: i32 = 1
    val r: &mut i32 = &mut x
    *r = 99
    return *r
}
"#;
    run_expecting(source, 99);
}

#[test]
fn a_mutable_reference_is_passed_where_a_shared_one_is_expected() {
    // A `&mut` handed to a `&` parameter is a shared reborrow for the call: a free
    // function, a generic one, a method argument, an associated function. The binding
    // stays writable afterwards, and the read after `bump` sees the write.
    let source = r#"
struct P { v: i32 }
impl P {
    func read(&self, other: &P) -> i32 { self.v + other.v }
    func make(p: &P) -> i32 { p.v }
}
func peek(x: &i32) -> i32 { *x }
func pick<T>(x: &T) -> i32 { 1 }
func bump(x: &mut i32) { *x = *x + 1 }
func h(w: &mut i32, p: &mut P) -> i32 {
    val a = peek(w) + pick(w) + p.read(p) + P::make(p)
    bump(w)
    a + peek(w) + 5
}
func main() -> i32 {
    mut x = 5
    mut p = P { v: 7 }
    h(&mut x, &mut p)
}
"#;
    run_expecting(source, 38);
}

/// A `&mut` moves: a plain binding takes it, so a second copy, which would be a second
/// exclusive borrow of the same value, is a use after move.
#[test]
fn regression_a_binding_moves_a_mutable_reference() {
    assert_check_error(
        "mut_ref_copied.nr",
        r#"
func main() -> i32 {
    mut n = 3
    val a = &mut n
    val b = a
    val c = a
    *b = 1
    *c = 2
    0
}
"#,
        "use of moved value 'a'",
    );
    assert_check_error(
        "mut_ref_copy_struct.nr",
        r#"
@derive(Copy)
struct P<T> { x: T }
func main() -> i32 {
    mut n = 3
    val a = P { x: &mut n }
    val b = a
    val c = a
    0
}
"#,
        "use of moved value 'a'",
    );
}

/// A reference parameter reborrows a `&mut` argument for the call, so the binding is
/// usable again once the call returns.
#[test]
fn a_mutable_reference_is_reborrowed_by_each_call() {
    run_expecting(
        r#"
func bump(r: &mut i32) { *r += 1 }
func peek(r: &i32) -> i32 { *r }
func id<T>(x: T) -> T { x }
struct S { v: Vec<i32> }
impl S { func add(&mut self, x: i32) { self.v.push(x) } }
func fill(s: &mut S, k: i32) { s.add(k) }
func main() -> i32 {
    mut n = 3
    mut v: Vec<i32> = Vec::new()
    mut s = S { v: Vec::new() }
    mut k = 0
    {
        val a = &mut n
        bump(a)
        bump(a)
        k = peek(a)
        *a += 1
        val w = &mut v
        w.push(1)
        w.push(2)
        val t = &mut s
        t.add(4)
        fill(t, 5)
        t.add(6)
        val u = id(t)
        u.add(7)
    }
    k + n + v.len() as i32 + s.v.len() as i32
}
"#,
        17,
    );
}
