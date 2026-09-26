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

/// A store into a field, an element or a tensor coordinate is refused while a shared
/// borrow of the binding it is rooted at is live, as a store into the whole binding is.
/// Only the whole-binding form consulted the borrow counts, so the view saw the write,
/// and a displaced owned value was freed under it.
#[test]
fn test_bug_070_a_sub_place_store_is_refused_while_the_root_is_borrowed() {
    let stores = [
        (
            "val view: &string = h.s.slice(0..7)",
            "h.s = make(2)",
            "println(view)",
            "h",
        ),
        ("val r = &a", "a[0] = 9", "println(\"{r[0]}\")", "a"),
        ("val r = &a", "a[0] += 5", "println(\"{r[0]}\")", "a"),
        ("val r = &p", "p.x = 5", "println(\"{r.x}\")", "p"),
        ("val r = &v", "v[0] = 3", "println(\"{r.len()}\")", "v"),
        ("val r = &t", "t[0] = 4.0", "println(\"{r[0]}\")", "t"),
        ("val r = &t", "t[0] += 4.0", "println(\"{r[0]}\")", "t"),
    ];
    for (borrow, store, read, root) in stores {
        let source = format!(
            r#"
struct Holder {{ s: string }}
struct P {{ x: i32 }}
func make(n: i32) -> string {{ "value {{n}}" }}
func main() -> i32 {{
    mut h = Holder {{ s: make(1) }}
    mut a: [i32; 2] = [1, 2]
    mut p = P {{ x: 1 }}
    mut t: Tensor<f32, [2]> = [1.0, 2.0]
    mut v: Vec<i32> = Vec::new()
    v.push(1)
    {borrow}
    {store}
    {read}
    0
}}
"#
        );
        let errors = semantic_errors(&source);
        assert!(
            errors.iter().any(
                |e| matches!(e, TypeError::CannotAssignWhileBorrowed { name, .. } if name == root)
            ),
            "`{store}` under `{borrow}` must be refused; got {errors:?}"
        );
    }
}

/// With no borrow live, the same stores are accepted.
#[test]
fn test_bug_070_a_sub_place_store_with_no_live_borrow_is_accepted() {
    let errors = semantic_errors(
        r#"
struct P { x: i32 }
func main() -> i32 {
    mut a: [i32; 2] = [1, 2]
    mut p = P { x: 1 }
    {
        val r = &a
        println("{r[0]}")
    }
    a[0] = 9
    p.x = 5
    a[0] + p.x
}
"#,
    );
    assert!(errors.is_empty(), "{errors:?}");
}

fn escapes(errors: &[TypeError]) -> usize {
    errors
        .iter()
        .filter(|e| matches!(e, TypeError::FunctionValueEscapes { .. }))
        .count()
}

/// A closure reads its captures from the frame that wrote it, so a function value may
/// not leave that frame: stored through a reference, or returned. Both were accepted, and
/// the stored or returned closure then read a dead frame (`2 * 11` for `2 * 3`).
#[test]
fn test_bug_072_a_function_value_may_not_leave_its_frame() {
    let prelude = r#"
struct Holder { f: (f32) -> f32 }
type F = (f32) -> f32
"#;
    let escaping = [
        "func keep(h: &mut Holder, g: (f32) -> f32) { h.f = g }",
        "func keep(s: &mut F, g: F) { *s = g }",
        "impl Holder { func set(&mut self, g: (f32) -> f32) { self.f = g } }",
        "func make(k: f32) -> (f32) -> f32 { |x: f32| -> f32 { x * k } }",
        "func make(k: f32) -> Holder { Holder { f: |x: f32| -> f32 { x * k } } }",
    ];
    for item in escaping {
        let errors = semantic_errors(&format!("{prelude}{item}\nfunc main() -> i32 {{ 0 }}\n"));
        assert_eq!(
            escapes(&errors),
            1,
            "`{item}` must be refused once; got {errors:?}"
        );
    }
}

/// Inside the frame that wrote it a function value may still be bound, stored into a
/// local holder, passed down and called.
#[test]
fn test_bug_072_a_function_value_used_in_its_own_frame_is_accepted() {
    let errors = semantic_errors(
        r#"
struct Holder { f: (f32) -> f32 }
func apply(g: (f32) -> f32, x: f32) -> f32 { g(x) }
func main() -> i32 {
    val k = 3.0f32
    mut h = Holder { f: |x: f32| -> f32 { x } }
    h.f = |x: f32| -> f32 { x * k }
    val f = h.f
    apply(f, 2.0) as i32
}
"#,
    );
    assert!(errors.is_empty(), "{errors:?}");
}

/// A callee cannot keep a function value any more, so a `pool` call handing one over
/// beside an outer `&mut` is not a retention.
#[test]
fn test_bug_072_a_pool_call_may_pass_a_closure_beside_an_outer_mut() {
    let errors = semantic_errors(
        r#"
func scale(w: &mut Tensor<f32, [2]>, g: (f32) -> f32) { w[0] = g(w[0]) }
func main() -> i32 {
    mut w: Tensor<f32, [2]> = [1.0, 2.0]
    pool {
        scale(&mut w, |x: f32| -> f32 { x * 2.0 })
    }
    w[0] as i32
}
"#,
    );
    assert!(errors.is_empty(), "{errors:?}");
}
