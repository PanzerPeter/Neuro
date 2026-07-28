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
