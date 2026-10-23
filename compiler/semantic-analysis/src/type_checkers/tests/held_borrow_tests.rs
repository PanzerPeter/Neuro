//! Borrows held inside a value: a struct field, an enum payload, `Option<&T>`, a tuple or
//! array element. The value's binding holds them, so the borrowee rules, the scope rule
//! and the returned-reference rule all see them.

use super::semantic_errors;
use crate::errors::TypeError;

/// `Option` is prelude source, which these programs do not load, so they declare it.
const DECLS: &str = "
enum Option<T> { Some(T), None }
struct H<'a> { r: &'a string }
enum E<'a, T> { A(&'a T), B(T) }
func first(s: &string) -> Option<&string> { Option::Some(s) }
func set(h: &mut H, s: &string) { h.r = s }
";

fn errors_of(body: &str) -> Vec<TypeError> {
    semantic_errors(&format!("{DECLS}{body}"))
}

fn outlives(errors: &[TypeError], place: &str) -> bool {
    errors
        .iter()
        .any(|e| matches!(e, TypeError::BorrowOutlivesPlace { place: p, .. } if p == place))
}

#[test]
fn a_struct_holding_a_borrow_freezes_its_borrowee() {
    let errors = errors_of(
        "func main() -> i32 {
    mut s = \"hi\"
    val h = H { r: &s }
    s = \"bye\"
    val t = s
    val m = &mut s
    h.r.len() as i32
}",
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::CannotAssignWhileBorrowed { name, .. } if name == "s")),
        "{errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::CannotMoveWhileBorrowed { name, .. } if name == "s")),
        "{errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::CannotMutablyBorrowWhileBorrowed { .. })),
        "{errors:?}"
    );
}

#[test]
fn every_holder_shape_takes_the_borrow() {
    // A variant constructor, a call result, a tuple, an array and a struct-variant
    // literal each hold `s`, so moving it afterwards is refused once per holder.
    for init in [
        "E::A(&s)",
        "first(&s)",
        "(&s, 1)",
        "[&s, &s]",
        "{ val inner = H { r: &s }\n inner }",
    ] {
        let errors = errors_of(&format!(
            "func main() -> i32 {{
    val s = \"hi\"
    val held = {init}
    val t = s
    0
}}"
        ));
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, TypeError::CannotMoveWhileBorrowed { .. })),
            "`val held = {init}` must keep `s` borrowed; got {errors:?}"
        );
    }
}

#[test]
fn a_holder_releases_its_borrow_with_its_scope() {
    let errors = errors_of(
        "func main() -> i32 {
    val s = \"hi\"
    {
        val h = H { r: &s }
    }
    val t = s
    t.len() as i32
}",
    );
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn a_holder_may_not_outlive_its_borrowee() {
    for store in ["h = H { r: &s }", "h.r = &s", "set(&mut h, &s)"] {
        let errors = errors_of(&format!(
            "func main() -> i32 {{
    val t = \"x\"
    mut h = H {{ r: &t }}
    {{
        val s = \"a\" + \"b\"
        {store}
    }}
    h.r.len() as i32
}}"
        ));
        assert!(
            outlives(&errors, "s"),
            "`{store}` must be refused; got {errors:?}"
        );
    }
}

#[test]
fn a_holder_in_the_borrowee_scope_is_accepted() {
    for store in ["h = H { r: &s }", "h.r = &s", "set(&mut h, &s)"] {
        let errors = errors_of(&format!(
            "func main() -> i32 {{
    val t = \"x\"
    val s = \"a\" + \"b\"
    mut h = H {{ r: &t }}
    {store}
    h.r.len() as i32
}}"
        ));
        assert!(
            errors.is_empty(),
            "`{store}` must be accepted; got {errors:?}"
        );
    }
}

#[test]
fn test_bug_109_a_reference_assigned_in_an_inner_block_is_refused() {
    let errors = semantic_errors(
        "func main() -> i32 {
    val t: Vec<i32> = Vec::new()
    mut b: &Vec<i32> = &t
    {
        mut v: Vec<i32> = Vec::new()
        v.push(41)
        b = &v
    }
    return b[0]
}",
    );
    assert!(outlives(&errors, "v"), "{errors:?}");
}

#[test]
fn a_pattern_binding_carries_the_scrutinee_borrow() {
    let errors = errors_of(
        "func main() -> i32 {
    val t = \"x\"
    mut out = &t
    {
        val s = \"a\" + \"b\"
        val o = Option::Some(&s)
        match o { Option::Some(r) => { out = r }, Option::None => {} }
    }
    out.len() as i32
}",
    );
    assert!(outlives(&errors, "s"), "{errors:?}");
}

#[test]
fn test_bug_099_a_struct_returned_through_a_binding_is_checked() {
    let errors = errors_of(
        "func make() -> H {
    val s = \"he\" + \"llo\"
    val h = H { r: &s }
    h
}
func main() -> i32 { 0 }",
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::ReturnsReferenceToLocal { name, .. } if name == "s")),
        "{errors:?}"
    );
}

#[test]
fn returning_what_a_local_holder_borrows_is_refused() {
    for tail in [
        "h.r",
        "o ?? &t",
        "match o { Option::Some(r) => r, Option::None => &t }",
        "s.slice(0..1)",
    ] {
        let errors = errors_of(&format!(
            "func pick(t: &string) -> &string {{
    val s = \"he\" + \"llo\"
    val h = H {{ r: &s }}
    val o = Option::Some(&s)
    {tail}
}}
func main() -> i32 {{ 0 }}"
        ));
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, TypeError::ReturnsReferenceToLocal { .. })),
            "returning `{tail}` must be refused; got {errors:?}"
        );
    }
}

#[test]
fn returning_a_holder_of_parameter_borrows_is_accepted() {
    let errors = errors_of(
        "func wrap<'a>(s: &'a string) -> H<'a> {
    val h = H { r: s }
    h
}
func pass(s: &string) -> Option<&string> {
    val o = first(s)
    o
}
func main() -> i32 {
    val s = \"hi\"
    val h = wrap(&s)
    val o = pass(&s)
    h.r.len() as i32 + (o ?? &s).len() as i32
}",
    );
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn storing_a_local_borrow_through_a_parameter_is_refused() {
    let errors = errors_of(
        "func fill(h: &mut H, r: &mut &string, keep: &string) {
    val s = \"a\" + \"b\"
    h.r = &s
    *r = &s
    h.r = keep
}
func main() -> i32 { 0 }",
    );
    let stores = errors
        .iter()
        .filter(|e| matches!(e, TypeError::StoresReferenceToLocal { name, .. } if name == "s"))
        .count();
    assert_eq!(stores, 2, "{errors:?}");
}

#[test]
fn a_type_lifetime_must_be_declared() {
    let errors = semantic_errors("struct H<'a> { r: &'b string }\nfunc main() -> i32 { 0 }");
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::UndeclaredLifetime { name, .. } if name == "b")),
        "{errors:?}"
    );
    let errors = semantic_errors("enum E<'a> { A(&'a string) }\nfunc main() -> i32 { 0 }");
    assert!(errors.is_empty(), "{errors:?}");
}
