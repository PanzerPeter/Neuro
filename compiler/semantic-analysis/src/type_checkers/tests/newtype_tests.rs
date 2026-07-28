#[allow(unused_imports)]
use super::{make_function, make_ident, make_type, semantic_errors};
use crate::errors::TypeError;

#[test]
fn newtype_construction_and_inner_access_type_check() {
    let errors = semantic_errors(
        r#"
newtype Meters = i32
func main() -> i32 {
    val m: Meters = Meters(7)
    val raw: i32 = m.0
    return raw
}
"#,
    );
    assert!(
        errors.is_empty(),
        "valid newtype use should check; got {errors:?}"
    );
}

#[test]
fn newtype_is_not_interchangeable_with_inner() {
    // Assigning a `Meters` where an `i32` is expected is a type error — a newtype is
    // a DISTINCT nominal type, unlike a transparent `type` alias.
    let errors = semantic_errors(
        r#"
newtype Meters = i32
func main() -> i32 {
    val m: Meters = Meters(7)
    val bad: i32 = m
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::Mismatch { .. })),
        "newtype must not be assignable to its inner type; got {errors:?}"
    );
}

#[test]
fn two_newtypes_over_the_same_inner_are_distinct() {
    let errors = semantic_errors(
        r#"
newtype Meters = i32
newtype Seconds = i32
func take(m: Meters) -> i32 { m.0 }
func main() -> i32 {
    return take(Seconds(3))
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::Mismatch { .. })),
        "passing Seconds where Meters is expected must be rejected; got {errors:?}"
    );
}

#[test]
fn newtype_construction_arity_is_enforced() {
    let errors = semantic_errors(
        r#"
newtype Meters = i32
func main() -> i32 {
    val m = Meters(1, 2)
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::ArgumentCountMismatch { .. })),
        "newtype construction takes exactly one argument; got {errors:?}"
    );
}

#[test]
fn newtype_over_non_copy_inner_is_rejected() {
    let errors = semantic_errors(
        r#"
newtype Name = string
func main() -> i32 { return 0 }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::NewtypeInnerNotCopy { .. })),
        "a newtype over a non-Copy inner type must be rejected; got {errors:?}"
    );
}

#[test]
fn cyclic_newtype_is_rejected() {
    let errors = semantic_errors(
        r#"
newtype A = B
newtype B = A
func main() -> i32 { return 0 }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::CyclicNewtype { .. })),
        "a cyclic newtype must be rejected; got {errors:?}"
    );
}

#[test]
fn newtype_shadowing_a_builtin_is_rejected() {
    let errors = semantic_errors(
        r#"
newtype i32 = i64
func main() -> i32 { return 0 }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::NewtypeAlreadyDefined { .. })),
        "a newtype may not shadow a builtin type name; got {errors:?}"
    );
}

#[test]
fn newtype_inner_index_out_of_range_is_rejected() {
    let errors = semantic_errors(
        r#"
newtype Meters = i32
func main() -> i32 {
    val m = Meters(5)
    return m.1
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::TupleIndexOutOfBounds { .. })),
        "a newtype has only index 0; `.1` must be rejected; got {errors:?}"
    );
}

// --- Generics ---
