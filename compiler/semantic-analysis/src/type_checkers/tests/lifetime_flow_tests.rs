//! A signature's explicit lifetimes say which arguments a call's result, or a `&mut`
//! argument's referent, may hold: a call is narrowed to them, and the callee's body is held
//! to them (LIM-238). A local stored into a `&mut` parameter's referent by an inner call is
//! refused like a direct store (BUG-172).
use super::semantic_errors;
use crate::errors::TypeError;

const HELPERS: &str = "struct S { x: i32 }
struct H<'a> { r: &'a i32 }
func pick<'a>(a: &'a i32, b: &i32) -> &'a i32 {
    if *b > 0 { a } else { a }
}
func set<'a>(h: &mut H<'a>, r: &'a i32, n: &i32) {
    if *n > 0 { h.r = r }
}
func set_any(h: &mut H, n: &i32) {}
impl<'a> H<'a> {
    func swap(&mut self, r: &'a i32, n: &i32) {
        if *n > 0 { self.r = r }
    }
}
";

fn errors_with(body: &str) -> Vec<TypeError> {
    semantic_errors(&format!("{HELPERS}{body}"))
}

#[test]
fn regression_a_call_result_holds_only_the_arguments_its_lifetime_names() {
    for body in [
        "func main() -> i32 {
    val x = 1
    mut n = 2
    val r = pick(&x, &n)
    n += 1
    *r
}",
        "func outer(p: &i32) -> &i32 {
    val local = 2
    pick(p, &local)
}
func main() -> i32 { 0 }",
        "func outer<'a>(p: &'a i32) -> &'a i32 {
    val local = 2
    return pick(p, &local)
}
func main() -> i32 { 0 }",
        "func main() -> i32 {
    val x = 1
    val r = {
        val local = 3
        pick(&x, &local)
    }
    *r
}",
        "func chained<'a>(a: &'a i32, b: &i32) -> &'a i32 { pick(a, b) }
func shared<'a>(a: &'a i32, b: &'a i32) -> &'a i32 { b }
func field<'a>(a: &'a S, b: &S) -> &'a i32 { &a.x }
func main() -> i32 { 0 }",
    ] {
        let errors = errors_with(body);
        assert!(errors.is_empty(), "{body}: {errors:?}");
    }
}

#[test]
fn regression_a_mut_argument_takes_only_the_arguments_its_lifetime_names() {
    for call in ["set(&mut h, &outer, &inner)", "h.swap(&outer, &inner)"] {
        let body = format!(
            "func main() -> i32 {{
    val outer = 1
    mut h = H {{ r: &outer }}
    {{
        val inner = 2
        {call}
    }}
    *h.r
}}"
        );
        let errors = errors_with(&body);
        assert!(errors.is_empty(), "{call}: {errors:?}");
    }
}

#[test]
fn regression_an_argument_its_lifetime_names_is_still_held() {
    let errors = errors_with(
        "func outer(p: &i32) -> &i32 {
    val local = 2
    pick(&local, p)
}
func main() -> i32 { 0 }",
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::ReturnsReferenceToLocal { .. })),
        "{errors:?}"
    );
    let errors = errors_with(
        "func main() -> i32 {
    val outer = 1
    mut h = H { r: &outer }
    {
        val inner = 2
        set(&mut h, &inner, &outer)
    }
    *h.r
}",
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::BorrowOutlivesPlace { .. })),
        "{errors:?}"
    );
}

fn unnamed_returns(errors: &[TypeError]) -> Vec<&str> {
    errors
        .iter()
        .filter_map(|e| match e {
            TypeError::ReturnsBorrowOfUnnamedLifetime { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn regression_a_body_returns_only_what_its_result_lifetime_names() {
    for (body, param) in [
        ("func bad<'a>(a: &'a i32, b: &i32) -> &'a i32 { b }", "b"),
        (
            "func bad<'a>(a: &'a i32, b: &i32) -> &'a i32 {
    val c = b
    c
}",
            "b",
        ),
        ("func bad<'a>(a: &'a S, b: &S) -> &'a i32 { &b.x }", "b"),
        (
            "func bad<'a>(a: &'a i32, b: &i32) -> &'a i32 { return pick(b, a) }",
            "b",
        ),
        (
            "func bad<'a>(a: &'a i32, b: &i32) -> H<'a> { H { r: b } }",
            "b",
        ),
    ] {
        let errors = errors_with(&format!("{body}\nfunc main() -> i32 {{ 0 }}"));
        assert_eq!(unnamed_returns(&errors), [param], "{body}: {errors:?}");
    }
}

fn unnamed_stores(errors: &[TypeError]) -> Vec<&str> {
    errors
        .iter()
        .filter_map(|e| match e {
            TypeError::StoresBorrowOfUnnamedLifetime { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn regression_a_body_stores_only_what_the_referent_lifetime_names() {
    for body in [
        "func bad<'a>(h: &mut H<'a>, r: &'a i32, n: &i32) { h.r = n }",
        "func bad<'a>(h: &mut H<'a>, r: &'a i32, n: &i32) { set_any(h, n) }",
        "func bad<'a>(h: &mut H<'a>, r: &'a i32, n: &i32) {
    val hh = h
    hh.r = n
}",
        "impl<'a> H<'a> {
    func bad(&mut self, r: &'a i32, n: &i32) { self.r = n }
}",
    ] {
        let errors = errors_with(&format!("{body}\nfunc main() -> i32 {{ 0 }}"));
        assert_eq!(unnamed_stores(&errors), ["n"], "{body}: {errors:?}");
    }
}

/// BUG-172: a call storing a local's borrow into the referent of a `&mut` parameter was
/// accepted, and the caller read the local's freed storage through its own value.
#[test]
fn regression_bug_172_a_call_may_not_store_a_local_through_a_mut_parameter() {
    for body in [
        "func fill(h: &mut H) {
    val local = 5
    set_any(h, &local)
}",
        "func fill(h: &mut H) {
    val local = 5
    val hh = h
    set_any(hh, &local)
}",
        "func fill<'a>(h: &mut H<'a>, keep: &'a i32) {
    val local = 5
    set(h, &local, keep)
}",
    ] {
        let errors = errors_with(&format!("{body}\nfunc main() -> i32 {{ 0 }}"));
        assert!(
            errors.iter().any(
                |e| matches!(e, TypeError::StoresReferenceToLocal { name, .. } if name == "local")
            ),
            "{body}: {errors:?}"
        );
    }
    // A lifetime the referent does not name keeps the local out of it.
    let errors = errors_with(
        "func fill<'a>(h: &mut H<'a>, keep: &'a i32) {
    val local = 5
    set(h, keep, &local)
}
func main() -> i32 { 0 }",
    );
    assert!(errors.is_empty(), "{errors:?}");
}

/// A closure's own result is not the function's, but a store it makes through a captured
/// parameter lands in the caller's storage all the same.
#[test]
fn regression_a_closure_returns_freely_and_stores_under_the_summary() {
    let errors = errors_with(
        "func ok<'a>(a: &'a i32, b: &i32) -> &'a i32 {
    val f = |x: &i32| -> i32 { *x + *b }
    val n = f(b)
    a
}
func main() -> i32 { 0 }",
    );
    assert!(errors.is_empty(), "{errors:?}");
    let errors = errors_with(
        "func bad<'a>(h: &mut H<'a>, r: &'a i32, n: &i32) {
    mut f = || { h.r = n }
    f()
}
func main() -> i32 { 0 }",
    );
    assert_eq!(unnamed_stores(&errors), ["n"], "{errors:?}");
}
