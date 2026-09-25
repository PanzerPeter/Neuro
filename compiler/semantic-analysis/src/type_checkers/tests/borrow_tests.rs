#[allow(unused_imports)]
use super::{make_function, make_ident, make_type, semantic_errors};
use crate::errors::TypeError;

fn is_borrow_conflict(error: &TypeError) -> bool {
    matches!(
        error,
        TypeError::CannotMutablyBorrowWhileBorrowed { .. }
            | TypeError::CannotBorrowWhileMutablyBorrowed { .. }
    )
}

fn returns_ref_to_local(errors: &[TypeError]) -> bool {
    errors
        .iter()
        .any(|e| matches!(e, TypeError::ReturnsReferenceToLocal { .. }))
}

#[test]
fn mutable_borrow_while_shared_borrow_is_live_is_rejected() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    mut x: i32 = 5
    val a: &i32 = &x
    val b: &mut i32 = &mut x
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::CannotMutablyBorrowWhileBorrowed { .. })),
        "a `&mut` while a `&` is live must be rejected; got {errors:?}"
    );
}

#[test]
fn second_mutable_borrow_is_rejected() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    mut x: i32 = 5
    val a: &mut i32 = &mut x
    val b: &mut i32 = &mut x
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::CannotMutablyBorrowWhileBorrowed { .. })),
        "a second `&mut` of the same place must be rejected; got {errors:?}"
    );
}

#[test]
fn shared_borrow_while_mutable_borrow_is_live_is_rejected() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    mut x: i32 = 5
    val a: &mut i32 = &mut x
    val b: &i32 = &x
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::CannotBorrowWhileMutablyBorrowed { .. })),
        "a `&` while a `&mut` is live must be rejected; got {errors:?}"
    );
}

#[test]
fn multiple_shared_borrows_coexist() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    mut x: i32 = 5
    val a: &i32 = &x
    val b: &i32 = &x
    return 0
}
"#,
    );
    assert!(
        !errors.iter().any(is_borrow_conflict),
        "any number of `&` borrows may coexist; got {errors:?}"
    );
}

#[test]
fn mutable_and_shared_borrow_in_one_call_is_rejected() {
    let errors = semantic_errors(
        r#"
func two(a: &mut i32, b: &i32) -> i32 { *a }
func main() -> i32 {
    mut x: i32 = 5
    val r: i32 = two(&mut x, &x)
    return r
}
"#,
    );
    assert!(
        errors.iter().any(is_borrow_conflict),
        "a `&mut` and a `&` of the same place in one call must conflict; got {errors:?}"
    );
}

#[test]
fn borrow_released_at_end_of_block_scope() {
    // The branch-scoped `&mut x` ends when the `if` body scope is left, so the
    // later `&mut x` is free to take its own exclusive borrow.
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    mut x: i32 = 5
    if true {
        val a: &mut i32 = &mut x
        *a = 7
    }
    val b: &mut i32 = &mut x
    *b = 9
    return 0
}
"#,
    );
    assert!(
        !errors.iter().any(is_borrow_conflict),
        "a borrow must be released at the end of its scope; got {errors:?}"
    );
}

#[test]
fn transient_borrows_in_separate_statements_do_not_conflict() {
    let errors = semantic_errors(
        r#"
func inc(n: &mut i32) { *n = *n + 1 }
func main() -> i32 {
    mut x: i32 = 5
    inc(&mut x)
    inc(&mut x)
    return x
}
"#,
    );
    assert!(
        !errors.iter().any(is_borrow_conflict),
        "a `&mut` passed to a call ends with the call; got {errors:?}"
    );
}

#[test]
fn reassigning_a_reference_releases_its_previous_borrow() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    mut x: i32 = 5
    mut y: i32 = 9
    mut r: &mut i32 = &mut x
    r = &mut y
    val b: &mut i32 = &mut x
    *b = 1
    return 0
}
"#,
    );
    assert!(
        !errors.iter().any(is_borrow_conflict),
        "reassigning `r` away from `x` frees `x` to be borrowed again; got {errors:?}"
    );
}

#[test]
fn returning_reference_to_local_is_rejected() {
    let errors = semantic_errors(
        r#"
func dangle() -> &i32 {
    val local: i32 = 5
    return &local
}
"#,
    );
    assert!(
        returns_ref_to_local(&errors),
        "borrowing a body-local and returning it dangles; got {errors:?}"
    );
}

#[test]
fn returning_reference_to_owned_parameter_is_rejected() {
    let errors = semantic_errors(
        r#"
func dangle(n: i32) -> &i32 {
    return &n
}
"#,
    );
    assert!(
        returns_ref_to_local(&errors),
        "a by-value parameter does not outlive the call; got {errors:?}"
    );
}

#[test]
fn returning_a_reference_parameter_is_accepted() {
    let errors = semantic_errors(
        r#"
func identity(r: &i32) -> &i32 {
    r
}
"#,
    );
    assert!(
        !returns_ref_to_local(&errors),
        "a reference parameter outlives the call (single-input elision); got {errors:?}"
    );
}

#[test]
fn returning_reference_through_local_binding_is_rejected() {
    let errors = semantic_errors(
        r#"
func leak() -> &i32 {
    val local: i32 = 7
    val r: &i32 = &local
    r
}
"#,
    );
    assert!(
        returns_ref_to_local(&errors),
        "a local reference binding that borrows a local dangles transitively; got {errors:?}"
    );
}

#[test]
fn returning_a_reference_in_an_if_arm_is_checked() {
    // The `else` arm yields a local reference binding whose borrowee is a body
    // local; the `then` arm yields the sound reference parameter. The walk into
    // both arms of the returned `if`-expression must still flag the bad arm.
    let errors = semantic_errors(
        r#"
func pick(cond: bool, r: &i32) -> &i32 {
    val local: i32 = 1
    val bad: &i32 = &local
    return if cond { r } else { bad }
}
"#,
    );
    assert!(
        returns_ref_to_local(&errors),
        "the dangling `else` arm must be caught even when another arm is sound; got {errors:?}"
    );
}

#[test]
fn returning_a_borrow_of_self_is_accepted() {
    // `&self` outlives the call, so a method may return a borrow of `self` (the
    // `&self` lifetime is applied to method outputs). Without `self` in the
    // outliving set this would be wrongly flagged as a local.
    let errors = semantic_errors(
        r#"
struct Wrapper { value: i32 }

impl Wrapper {
    func me(&self) -> &Wrapper {
        return &self
    }
}
"#,
    );
    assert!(
        !returns_ref_to_local(&errors),
        "a borrow of `&self` outlives the call; got {errors:?}"
    );
}

/// A `&mut` binding handed to one call twice gives the callee two live `&mut` of one place.
/// The owner spelling `g(&mut x, &mut x)` was already refused; the reborrow spelling, and
/// the same binding as both receiver and argument, were accepted and let the callee observe
/// the aliasing.
#[test]
fn test_bug_065_one_mutable_reborrow_per_call() {
    let conflicting = [
        r#"
func g(a: &mut i32, b: &mut i32) -> i32 { *a }
func h(w: &mut i32) -> i32 { g(w, w) }
func main() -> i32 { 0 }
"#,
        r#"
func g(a: &i32, b: &mut i32) -> i32 { *a }
func h(w: &mut i32) -> i32 { g(w, w) }
func main() -> i32 { 0 }
"#,
        r#"
struct C { v: i32 }
impl C {
    func take(&mut self, other: &mut C) -> i32 { self.v }
}
func h(w: &mut C) -> i32 { w.take(w) }
func main() -> i32 { 0 }
"#,
    ];
    for source in conflicting {
        let errors = semantic_errors(source);
        assert!(
            errors.iter().any(is_borrow_conflict),
            "two reborrows of one `&mut`, one of them exclusive, must be rejected; got {errors:?}\n{source}"
        );
    }
    let sequential = r#"
func g(a: &mut i32) -> i32 { *a }
func r(a: &i32, b: &i32) -> i32 { *a + *b }
func h(w: &mut i32) -> i32 { g(w) + g(w) + r(w, w) }
func main() -> i32 { 0 }
"#;
    let errors = semantic_errors(sequential);
    assert!(
        errors.is_empty(),
        "reborrows in separate calls, or two shared ones, do not overlap; got {errors:?}"
    );
}

/// A `&mut T` argument at a `&T` parameter is a shared reborrow for the call, as the
/// `@grad` examples of the language reference assume (a differentiated parameter handed
/// to a helper that only reads it).
#[test]
fn a_mutable_reference_is_accepted_at_a_shared_parameter() {
    let errors = semantic_errors(
        r#"
struct P { v: i32 }
impl P {
    func read(&self, other: &P) -> i32 { self.v + other.v }
    func make(p: &P) -> i32 { p.v }
}
func peek(x: &i32) -> i32 { *x }
func pick<T>(x: &T) -> i32 { 1 }
func h(w: &mut i32, p: &mut P) -> i32 { peek(w) + pick(w) + p.read(p) + P::make(p) }
func main() -> i32 { 0 }
"#,
    );
    assert!(
        errors.is_empty(),
        "a `&mut` argument coerces to a `&` parameter; got {errors:?}"
    );
}
